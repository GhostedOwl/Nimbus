mod config;
mod forecast_window;
mod geo;
mod settings_window;
mod tray;
mod weather;

use anyhow::Result;
use config::{Config, TrayTemp};
use forecast_window::ForecastWindow;
use settings_window::SettingsWindow;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tray::Tray;
use tray_icon::{TrayIconEvent, MouseButton};
use weather::WeatherData;
use winit::{
    application::ApplicationHandler,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    window::{Window, WindowId},
};

// ─── User events (cross-thread signalling) ────────────────────────────────────

#[derive(Debug)]
enum AppEvent {
    // From tray
    TrayLeftClick,
    MenuSettings,
    MenuRefresh,
    MenuQuit,
    // From async weather task
    WeatherReady(Box<WeatherData>),
    WeatherError(String),
    // From settings window
    ConfigSaved(Config),
    // Internal: periodic refresh tick
    RefreshTick,
}

// ─── Egui window state ────────────────────────────────────────────────────────

/// Wraps a winit Window + egui Context + egui_winit State for one popup window
struct EguiWindow {
    window: Arc<Window>,
    egui_ctx: egui::Context,
    egui_state: egui_winit::State,
    painter: egui_wgpu::winit::Painter,
}

impl EguiWindow {
    fn new(event_loop: &ActiveEventLoop, title: &str, size: [f32; 2]) -> Result<Self> {
        use winit::window::WindowAttributes;
        let attrs = WindowAttributes::default()
            .with_title(title)
            .with_inner_size(winit::dpi::LogicalSize::new(size[0], size[1]))
            .with_resizable(false)
            .with_decorations(true)
            .with_visible(false); // start hidden

        let window = Arc::new(event_loop.create_window(attrs)?);
        let egui_ctx = egui::Context::default();
        let egui_state = egui_winit::State::new(
            egui_ctx.clone(),
            egui_ctx.viewport_id(),
            &window,
            None,
            None,
            None,
        );

        let painter = egui_wgpu::winit::Painter::new(
            egui_wgpu::WgpuConfiguration::default(),
            1,
            None,
            false,
            true,
        );

        // Painter needs to know about the surface — we init it lazily on first paint
        Ok(Self {
            window,
            egui_ctx,
            egui_state,
            painter,
        })
    }

    fn show(&self) {
        self.window.set_visible(true);
        self.window.focus_window();
    }

    fn hide(&self) {
        self.window.set_visible(false);
    }

    fn id(&self) -> WindowId {
        self.window.id()
    }

    fn on_window_event(&mut self, event: &WindowEvent) -> bool {
        let resp = self.egui_state.on_window_event(&self.window, event);
        if resp.repaint {
            self.window.request_redraw();
        }
        resp.consumed
    }

    fn paint<F>(&mut self, build_ui: F)
    where
        F: FnMut(&egui::Context),
    {
        let raw_input = self.egui_state.take_egui_input(&self.window);
        let full_output = self.egui_ctx.run(raw_input, build_ui);

        self.egui_state
            .handle_platform_output(&self.window, full_output.platform_output);

        let tris = self.egui_ctx.tessellate(full_output.shapes, full_output.pixels_per_point);
        self.painter.paint_and_update_textures(
            full_output.pixels_per_point,
            egui::Rgba::BLACK,
            &tris,
            &full_output.textures_delta,
            false,
        );
    }
}

// ─── Refresh timer ────────────────────────────────────────────────────────────

struct RefreshTimer {
    interval: Duration,
    last: Instant,
}

impl RefreshTimer {
    fn new(minutes: u64) -> Self {
        Self {
            interval: Duration::from_secs(minutes * 60),
            // Trigger immediately on first check
            last: Instant::now() - Duration::from_secs(minutes * 60 + 1),
        }
    }

    fn is_due(&self) -> bool {
        self.last.elapsed() >= self.interval
    }

    fn reset(&mut self) {
        self.last = Instant::now();
    }

    fn set_interval_minutes(&mut self, minutes: u64) {
        self.interval = Duration::from_secs(minutes * 60);
    }
}

// ─── Main application state ───────────────────────────────────────────────────

struct NimbusApp {
    // Core state
    config: Config,
    weather: Option<WeatherData>,
    tray: Option<Tray>,

    // Windows (created lazily on first open)
    forecast_win: Option<EguiWindow>,
    settings_win: Option<EguiWindow>,

    // UI state
    forecast_ui: ForecastWindow,
    settings_ui: SettingsWindow,

    // Async
    rt: tokio::runtime::Handle,
    proxy: winit::event_loop::EventLoopProxy<AppEvent>,

    // Refresh scheduling
    refresh_timer: RefreshTimer,
    fetching: bool,
}

impl NimbusApp {
    fn new(
        config: Config,
        rt: tokio::runtime::Handle,
        proxy: winit::event_loop::EventLoopProxy<AppEvent>,
    ) -> Self {
        let refresh_timer = RefreshTimer::new(config.refresh_interval.minutes());
        let settings_ui = SettingsWindow::new(&config);
        let forecast_ui = ForecastWindow::new();

        Self {
            config,
            weather: None,
            tray: None,
            forecast_win: None,
            settings_win: None,
            forecast_ui,
            settings_ui,
            rt,
            proxy,
            refresh_timer,
            fetching: false,
        }
    }

    fn spawn_weather_fetch(&mut self) {
        if self.fetching {
            return;
        }
        self.fetching = true;
        self.refresh_timer.reset();

        let lat = self.config.latitude;
        let lon = self.config.longitude;
        let proxy = self.proxy.clone();

        self.rt.spawn(async move {
            match weather::fetch_weather(lat, lon).await {
                Ok(data) => {
                    let _ = proxy.send_event(AppEvent::WeatherReady(Box::new(data)));
                }
                Err(e) => {
                    log::error!("Weather fetch failed: {e}");
                    let _ = proxy.send_event(AppEvent::WeatherError(e.to_string()));
                }
            }
        });
    }

    fn tray_tooltip(&self) -> String {
        match &self.weather {
            None => "Nimbus — loading…".to_string(),
            Some(w) => format!(
                "Nimbus — {}\n{}, {:.0}°C / feels {:.0}°C\n💧 {:.1} mm  💨 {:.0} km/h {}",
                self.config.city_name,
                w.current.description(),
                w.current.temperature,
                w.current.feels_like,
                w.current.precipitation,
                w.current.wind_speed,
                w.current.wind_dir_label(),
            ),
        }
    }

    fn on_weather_ready(&mut self, data: WeatherData) {
        self.fetching = false;
        self.weather = Some(data.clone());

        // Update tray tooltip
        if let Some(tray) = &self.tray {
            tray.set_tooltip(&self.tray_tooltip());
        }

        // Push new data into forecast window
        self.forecast_ui.weather = Some(data);
        self.forecast_ui.city_name = self.config.city_name.clone();

        // Trigger repaint if window is open
        if let Some(win) = &self.forecast_win {
            win.window.request_redraw();
        }
    }

    fn on_config_saved(&mut self, new_config: Config) {
        log::info!("Config updated: {}", new_config.city_name);
        self.config = new_config.clone();

        if let Err(e) = self.config.save() {
            log::error!("Failed to save config: {e}");
        }

        self.refresh_timer.set_interval_minutes(self.config.refresh_interval.minutes());
        self.fetching = false; // allow immediate re-fetch
        self.spawn_weather_fetch();

        // Close settings window
        if let Some(win) = &self.settings_win {
            win.hide();
        }
    }

    fn toggle_forecast(&mut self, event_loop: &ActiveEventLoop) {
        if self.forecast_win.is_none() {
            match EguiWindow::new(event_loop, "Nimbus — Forecast", [540.0, 320.0]) {
                Ok(win) => self.forecast_win = Some(win),
                Err(e) => {
                    log::error!("Failed to create forecast window: {e}");
                    return;
                }
            }
        }
        if let Some(win) = &self.forecast_win {
            let visible = win.window.is_visible().unwrap_or(false);
            if visible {
                win.hide();
            } else {
                win.show();
                win.window.request_redraw();
            }
        }
    }

    fn open_settings(&mut self, event_loop: &ActiveEventLoop) {
        if self.settings_win.is_none() {
            match EguiWindow::new(event_loop, "Nimbus — Settings", [380.0, 280.0]) {
                Ok(win) => self.settings_win = Some(win),
                Err(e) => {
                    log::error!("Failed to create settings window: {e}");
                    return;
                }
            }
        }
        self.settings_ui.sync_from_config(&self.config);
        if let Some(win) = &self.settings_win {
            win.show();
            win.window.request_redraw();
        }
    }
}

// ─── winit ApplicationHandler impl ───────────────────────────────────────────

impl ApplicationHandler<AppEvent> for NimbusApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        // Build tray on first resume (platform requires event loop to be running)
        if self.tray.is_none() {
            match Tray::build() {
                Ok(t) => self.tray = Some(t),
                Err(e) => log::error!("Failed to build tray: {e}"),
            }
        }

        // Kick off initial weather fetch
        self.spawn_weather_fetch();

        // Poll for refresh ticks (winit doesn't give us a timer, so we use
        // ControlFlow::WaitUntil with a short timeout)
        event_loop.set_control_flow(ControlFlow::WaitUntil(
            Instant::now() + Duration::from_secs(30),
        ));
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
        // Route event to the right window
        let is_forecast = self
            .forecast_win
            .as_ref()
            .map(|w| w.id() == window_id)
            .unwrap_or(false);
        let is_settings = self
            .settings_win
            .as_ref()
            .map(|w| w.id() == window_id)
            .unwrap_or(false);

        match &event {
            WindowEvent::CloseRequested => {
                if is_forecast {
                    if let Some(w) = &self.forecast_win {
                        w.hide();
                    }
                } else if is_settings {
                    if let Some(w) = &self.settings_win {
                        w.hide();
                    }
                }
            }
            WindowEvent::RedrawRequested => {
                if is_forecast {
                    self.paint_forecast();
                } else if is_settings {
                    self.paint_settings();
                }
            }
            _ => {
                if is_forecast {
                    if let Some(w) = &mut self.forecast_win {
                        w.on_window_event(&event);
                    }
                } else if is_settings {
                    if let Some(w) = &mut self.settings_win {
                        w.on_window_event(&event);
                    }
                }
            }
        }

        // Poll tray menu events every window event (cheap)
        self.poll_tray_events(event_loop);

        // Schedule next refresh check
        event_loop.set_control_flow(ControlFlow::WaitUntil(
            Instant::now() + Duration::from_secs(30),
        ));
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: AppEvent) {
        match event {
            AppEvent::TrayLeftClick => self.toggle_forecast(event_loop),
            AppEvent::MenuSettings => self.open_settings(event_loop),
            AppEvent::MenuRefresh => self.spawn_weather_fetch(),
            AppEvent::MenuQuit => event_loop.exit(),
            AppEvent::WeatherReady(data) => self.on_weather_ready(*data),
            AppEvent::WeatherError(msg) => {
                log::error!("Weather error: {msg}");
                self.fetching = false;
            }
            AppEvent::ConfigSaved(cfg) => self.on_config_saved(cfg),
            AppEvent::RefreshTick => {
                if self.refresh_timer.is_due() {
                    self.spawn_weather_fetch();
                }
            }
        }

        // Repaint open windows
        if let Some(w) = &self.forecast_win {
            if w.window.is_visible().unwrap_or(false) {
                w.window.request_redraw();
            }
        }
        if let Some(w) = &self.settings_win {
            if w.window.is_visible().unwrap_or(false) {
                w.window.request_redraw();
            }
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        // Periodic refresh tick
        if self.refresh_timer.is_due() && !self.fetching {
            self.spawn_weather_fetch();
        }

        // Poll tray menu events (they don't wake the event loop on all platforms)
        self.poll_tray_events(event_loop);

        event_loop.set_control_flow(ControlFlow::WaitUntil(
            Instant::now() + Duration::from_secs(30),
        ));
    }
}

// ─── Paint helpers (separate so we can borrow self partially) ─────────────────

impl NimbusApp {
    fn paint_forecast(&mut self) {
        let forecast_ui = &mut self.forecast_ui;
        if let Some(win) = &mut self.forecast_win {
            win.paint(|ctx| {
                forecast_ui.show(ctx);
            });
        }
    }

    fn paint_settings(&mut self) {
        let settings_ui = &mut self.settings_ui;
        let proxy = self.proxy.clone();
        let rt = self.rt.clone();

        if let Some(win) = &mut self.settings_win {
            win.paint(|ctx| {
                settings_ui.show(ctx, &rt);

                // Forward pending config out via event
                if let Some(cfg) = settings_ui.pending_config.take() {
                    let _ = proxy.send_event(AppEvent::ConfigSaved(cfg));
                }
            });
        }
    }

    fn poll_tray_events(&mut self, event_loop: &ActiveEventLoop) {
        let tray = match &self.tray {
            Some(t) => t,
            None => return,
        };

        // Menu events
        while let Some(ev) = tray.poll_menu() {
            if ev.id == tray.id_settings {
                self.open_settings(event_loop);
            } else if ev.id == tray.id_refresh {
                self.spawn_weather_fetch();
            } else if ev.id == tray.id_quit {
                event_loop.exit();
            }
        }

        // Tray icon click events
        while let Ok(ev) = TrayIconEvent::receiver().try_recv() {
            if let TrayIconEvent::Click { button: MouseButton::Left, .. } = ev {
                self.toggle_forecast(event_loop);
            }
        }
    }
}

// ─── Entry point ─────────────────────────────────────────────────────────────

fn main() -> Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    // Tokio runtime for async HTTP
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;

    // Load or create config
    let config = match Config::load()? {
        Some(cfg) => {
            log::info!("Loaded config for: {}", cfg.city_name);
            cfg
        }
        None => {
            log::info!("No config — detecting location via IP…");
            let loc = rt.block_on(geo::detect_location())?;
            let cfg = Config {
                city_name: loc.city,
                latitude: loc.lat,
                longitude: loc.lon,
                tray_temp: TrayTemp::default(),
                refresh_interval: config::RefreshInterval::default(),
            };
            cfg.save()?;
            cfg
        }
    };

    let event_loop = EventLoop::<AppEvent>::with_user_event().build()?;
    let proxy = event_loop.create_proxy();

    // Forward tray click events into our event loop
    {
        let proxy = proxy.clone();
        TrayIconEvent::set_event_handler(Some(move |ev: TrayIconEvent| {
            if let TrayIconEvent::Click { button: MouseButton::Left, .. } = ev {
                let _ = proxy.send_event(AppEvent::TrayLeftClick);
            }
        }));
    }

    let mut app = NimbusApp::new(config, rt.handle().clone(), proxy);

    event_loop.run_app(&mut app)?;

    Ok(())
}
