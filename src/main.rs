#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod config;
mod forecast_window;
mod geo;
mod settings_window;
mod tray;
mod weather;

use anyhow::Result;
use config::{Config, TrayTemp};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tray::Tray;
use tray_icon::{TrayIconEvent, MouseButton};
use weather::WeatherData;
use winit::{
    application::ApplicationHandler,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    window::WindowId,
};

#[derive(Debug)]
enum AppEvent {
    TrayLeftClick,
    MenuSettings,
    MenuRefresh,
    MenuQuit,
    WeatherReady(Box<WeatherData>),
    WeatherError(String),
    ConfigSaved(Config),
}

struct NimbusApp {
    config: Arc<Mutex<Config>>,
    weather: Arc<Mutex<Option<WeatherData>>>,
    tray: Option<Tray>,
    rt: tokio::runtime::Handle,
    proxy: winit::event_loop::EventLoopProxy<AppEvent>,
    refresh_timer: Instant,
    fetching: bool,
}

impl NimbusApp {
    fn new(
        config: Config,
        rt: tokio::runtime::Handle,
        proxy: winit::event_loop::EventLoopProxy<AppEvent>,
    ) -> Self {
        Self {
            config: Arc::new(Mutex::new(config)),
            weather: Arc::new(Mutex::new(None)),
            tray: None,
            rt,
            proxy,
            refresh_timer: Instant::now() - Duration::from_secs(9999),
            fetching: false,
        }
    }

    fn spawn_fetch(&mut self) {
        if self.fetching { return; }
        self.fetching = true;
        self.refresh_timer = Instant::now();
        let (lat, lon) = {
            let c = self.config.lock().unwrap();
            (c.latitude, c.longitude)
        };
        let proxy = self.proxy.clone();
        self.rt.spawn(async move {
            match weather::fetch_weather(lat, lon).await {
                Ok(d) => { let _ = proxy.send_event(AppEvent::WeatherReady(Box::new(d))); }
                Err(e) => { let _ = proxy.send_event(AppEvent::WeatherError(e.to_string())); }
            }
        });
    }

    fn update_tray_label(&self) {
        let Some(tray) = &self.tray else { return };
        let w = self.weather.lock().unwrap();
        let c = self.config.lock().unwrap();
        let tooltip = match w.as_ref() {
            None => "Nimbus — loading…".to_string(),
            Some(w) => {
                let temp = match c.tray_temp {
                    TrayTemp::FeelsLike => w.current.feels_like,
                    TrayTemp::Actual => w.current.temperature,
                };
                let sign = if temp >= 0.0 { "+" } else { "" };
                format!("Nimbus — {}\n{}{:.0}°C / feels {:.0}°C",
                    c.city_name, sign, temp, w.current.feels_like)
            }
        };
        tray.set_tooltip(&tooltip);
    }

    fn open_forecast(&self) {
        let w = self.weather.lock().unwrap().clone();
        let city = self.config.lock().unwrap().city_name.clone();
        let Some(data) = w else { return };
        let app = forecast_window::ForecastApp { data, city_name: city };
        std::thread::spawn(move || {
            let opts = eframe::NativeOptions {
                viewport: egui::ViewportBuilder::default()
                    .with_title("Nimbus")
                    .with_inner_size([560.0, 320.0])
                    .with_resizable(false),
                ..Default::default()
            };
            let _ = eframe::run_native("Nimbus", opts,
                Box::new(|_cc| Ok(Box::new(app))));
        });
    }

    fn open_settings(&self) {
        let cfg = self.config.lock().unwrap().clone();
        let proxy = self.proxy.clone();
        let rt = self.rt.clone();
        std::thread::spawn(move || {
            let app = settings_window::SettingsApp::new(cfg, rt, proxy);
            let opts = eframe::NativeOptions {
                viewport: egui::ViewportBuilder::default()
                    .with_title("Nimbus — Settings")
                    .with_inner_size([360.0, 300.0])
                    .with_resizable(false),
                ..Default::default()
            };
            let _ = eframe::run_native("Nimbus — Settings", opts,
                Box::new(|_cc| Ok(Box::new(app))));
        });
    }

    fn poll_tray(&mut self, event_loop: &ActiveEventLoop) {
        let Some(tray) = &self.tray else { return };
        while let Some(ev) = tray.poll_menu() {
            if ev.id == tray.id_settings { self.open_settings(); }
            else if ev.id == tray.id_refresh { self.spawn_fetch(); }
            else if ev.id == tray.id_quit { event_loop.exit(); }
        }
        while let Ok(ev) = TrayIconEvent::receiver().try_recv() {
            if let TrayIconEvent::Click { button: MouseButton::Left, .. } = ev {
                self.open_forecast();
            }
        }
    }
}

impl ApplicationHandler<AppEvent> for NimbusApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.tray.is_none() {
            match Tray::build() {
                Ok(t) => self.tray = Some(t),
                Err(e) => log::error!("Tray failed: {e}"),
            }
        }
        self.spawn_fetch();
        event_loop.set_control_flow(ControlFlow::WaitUntil(Instant::now() + Duration::from_secs(30)));
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        if let WindowEvent::RedrawRequested = event {}
        self.poll_tray(event_loop);
        event_loop.set_control_flow(ControlFlow::WaitUntil(Instant::now() + Duration::from_secs(30)));
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: AppEvent) {
        match event {
            AppEvent::TrayLeftClick => self.open_forecast(),
            AppEvent::MenuSettings => self.open_settings(),
            AppEvent::MenuRefresh => self.spawn_fetch(),
            AppEvent::MenuQuit => event_loop.exit(),
            AppEvent::WeatherReady(data) => {
                self.fetching = false;
                *self.weather.lock().unwrap() = Some(*data);
                self.update_tray_label();
            }
            AppEvent::WeatherError(e) => {
                self.fetching = false;
                log::error!("Weather fetch failed: {e}");
            }
            AppEvent::ConfigSaved(cfg) => {
                *self.config.lock().unwrap() = cfg;
                self.fetching = false;
                self.spawn_fetch();
            }
        }
        event_loop.set_control_flow(ControlFlow::WaitUntil(Instant::now() + Duration::from_secs(30)));
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        let interval = {
            let c = self.config.lock().unwrap();
            Duration::from_secs(c.refresh_interval.minutes() * 60)
        };
        if self.refresh_timer.elapsed() >= interval {
            self.spawn_fetch();
        }
        self.poll_tray(event_loop);
        event_loop.set_control_flow(ControlFlow::WaitUntil(Instant::now() + Duration::from_secs(30)));
    }
}

fn main() -> Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let rt = tokio::runtime::Builder::new_multi_thread().enable_all().build()?;

    let config = match Config::load()? {
        Some(cfg) => cfg,
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
