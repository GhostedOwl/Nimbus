#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod config;
mod forecast_window;
mod geo;
mod settings_window;
mod tray;
mod weather;

use anyhow::Result;
use config::{Config, TrayTemp};
use eframe::egui;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tray::Tray;
use tray_icon::{TrayIconEvent, MouseButton};
use weather::WeatherData;

#[derive(Debug, Clone)]
enum AppEvent {
    WeatherReady(Box<WeatherData>),
    WeatherError(String),
    ConfigSaved(Config),
}

struct NimbusApp {
    config: Config,
    weather: Option<WeatherData>,
    tray: Option<Tray>,
    rt: tokio::runtime::Handle,
    event_rx: std::sync::mpsc::Receiver<AppEvent>,
    event_tx: std::sync::mpsc::Sender<AppEvent>,
    refresh_timer: Instant,
    fetching: bool,
    show_forecast: bool,
    show_settings: bool,
    forecast_ui: forecast_window::ForecastUi,
    settings_ui: settings_window::SettingsUi,
}

impl NimbusApp {
    fn new(config: Config, rt: tokio::runtime::Handle) -> Self {
        let (tx, rx) = std::sync::mpsc::channel();
        let forecast_ui = forecast_window::ForecastUi::new();
        let settings_ui = settings_window::SettingsUi::new(&config, rt.clone(), tx.clone());
        Self {
            config,
            weather: None,
            tray: None,
            rt,
            event_rx: rx,
            event_tx: tx,
            refresh_timer: Instant::now() - Duration::from_secs(9999),
            fetching: false,
            show_forecast: false,
            show_settings: false,
            forecast_ui,
            settings_ui,
        }
    }

    fn spawn_fetch(&mut self, ctx: &egui::Context) {
        if self.fetching { return; }
        self.fetching = true;
        self.refresh_timer = Instant::now();
        let lat = self.config.latitude;
        let lon = self.config.longitude;
        let tx = self.event_tx.clone();
        let ctx = ctx.clone();
        self.rt.spawn(async move {
            match weather::fetch_weather(lat, lon).await {
                Ok(d) => { let _ = tx.send(AppEvent::WeatherReady(Box::new(d))); }
                Err(e) => { let _ = tx.send(AppEvent::WeatherError(e.to_string())); }
            }
            ctx.request_repaint();
        });
    }

    fn update_tray(&self) {
        let Some(tray) = &self.tray else { return };
        let tooltip = match &self.weather {
            None => "Nimbus — loading…".to_string(),
            Some(w) => {
                let temp = match self.config.tray_temp {
                    TrayTemp::FeelsLike => w.current.feels_like,
                    TrayTemp::Actual    => w.current.temperature,
                };
                let sign = if temp >= 0.0 { "+" } else { "" };
                format!("Nimbus — {}\n{}{:.0}°C / feels {:.0}°C\n{}",
                    self.config.city_name, sign, temp, w.current.feels_like,
                    weather::wmo_description(w.current.weather_code))
            }
        };
        tray.set_tooltip(&tooltip);
        if let Some(w) = &self.weather {
            tray.set_weather_icon(w.current.weather_code);
        }
    }
}

impl eframe::App for NimbusApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Init tray once
        if self.tray.is_none() {
            if let Ok(t) = Tray::build() {
                self.tray = Some(t);
            }
        }

        // Process async events
        while let Ok(ev) = self.event_rx.try_recv() {
            match ev {
                AppEvent::WeatherReady(data) => {
                    self.fetching = false;
                    self.forecast_ui.weather = Some(*data.clone());
                    self.forecast_ui.city_name = self.config.city_name.clone();
                    self.weather = Some(*data);
                    self.update_tray();
                }
                AppEvent::WeatherError(e) => {
                    self.fetching = false;
                    log::error!("Weather fetch failed: {e}");
                }
                AppEvent::ConfigSaved(cfg) => {
                    self.config = cfg.clone();
                    self.settings_ui.sync_from_config(&cfg);
                    self.show_settings = false;
                    self.fetching = false;
                    self.spawn_fetch(ctx);
                }
            }
        }

        // Poll tray events
        {
            let mut open_forecast = false;
            let mut open_settings = false;
            let mut do_refresh = false;
            let mut do_quit = false;

            if let Some(tray) = &self.tray {
                let (id_s, id_r, id_q) = (tray.id_settings.clone(), tray.id_refresh.clone(), tray.id_quit.clone());
                while let Some(ev) = tray.poll_menu() {
                    if ev.id == id_s { open_settings = true; }
                    else if ev.id == id_r { do_refresh = true; }
                    else if ev.id == id_q { do_quit = true; }
                }
            }
            while let Ok(ev) = TrayIconEvent::receiver().try_recv() {
                if let TrayIconEvent::Click { button: MouseButton::Left, .. } = ev {
                    open_forecast = true;
                }
            }

            if open_forecast { self.show_forecast = true; }
            if open_settings { self.show_settings = true; }
            if do_refresh { self.spawn_fetch(ctx); }
            if do_quit { ctx.send_viewport_cmd(egui::ViewportCommand::Close); }
        }

        // Auto-refresh
        let interval = Duration::from_secs(self.config.refresh_interval.minutes() * 60);
        if self.refresh_timer.elapsed() >= interval {
            self.spawn_fetch(ctx);
        }

        // Keep main window hidden (we're a tray app)
        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
        ctx.request_repaint_after(Duration::from_secs(1));

        // Forecast viewport
        if self.show_forecast {
            let forecast_ui = Arc::new(Mutex::new(self.forecast_ui.clone()));
            let close_flag = Arc::new(Mutex::new(false));
            let close_flag2 = Arc::clone(&close_flag);
            ctx.show_viewport_immediate(
                egui::ViewportId::from_hash_of("forecast"),
                egui::ViewportBuilder::default()
                    .with_title("Nimbus")
                    .with_inner_size([560.0, 320.0])
                    .with_resizable(false),
                move |ctx, _| {
                    forecast_ui.lock().unwrap().show(ctx);
                    if ctx.input(|i| i.viewport().close_requested()) {
                        *close_flag2.lock().unwrap() = true;
                    }
                },
            );
            if *close_flag.lock().unwrap() {
                self.show_forecast = false;
            }
        }

        // Settings viewport
        if self.show_settings {
            let settings_ui = Arc::new(Mutex::new(self.settings_ui.clone()));
            let close_flag = Arc::new(Mutex::new(false));
            let close_flag2 = Arc::clone(&close_flag);
            ctx.show_viewport_immediate(
                egui::ViewportId::from_hash_of("settings"),
                egui::ViewportBuilder::default()
                    .with_title("Nimbus — Settings")
                    .with_inner_size([360.0, 300.0])
                    .with_resizable(false),
                move |ctx, _| {
                    settings_ui.lock().unwrap().show(ctx);
                    if ctx.input(|i| i.viewport().close_requested()) {
                        *close_flag2.lock().unwrap() = true;
                    }
                },
            );
            if *close_flag.lock().unwrap() {
                self.show_settings = false;
            }
        }
    }
}

fn main() -> Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    let rt = tokio::runtime::Builder::new_multi_thread().enable_all().build()?;

    let config = match Config::load()? {
        Some(cfg) => cfg,
        None => {
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

    let app = NimbusApp::new(config, rt.handle().clone());

    let opts = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1.0, 1.0])
            .with_decorations(false)
            .with_transparent(true),
        ..Default::default()
    };

    eframe::run_native("Nimbus", opts, Box::new(|_cc| Ok(Box::new(app))))?;
    Ok(())
}
