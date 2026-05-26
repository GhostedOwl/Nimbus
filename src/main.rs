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
pub enum AppEvent {
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
    forecast_ui: Arc<Mutex<forecast_window::ForecastUi>>,
    settings_ui: Arc<Mutex<settings_window::SettingsUi>>,
}

impl NimbusApp {
    fn new(config: Config, rt: tokio::runtime::Handle) -> Self {
        let (tx, rx) = std::sync::mpsc::channel();
        let forecast_ui = Arc::new(Mutex::new(forecast_window::ForecastUi::new()));
        let settings_ui = Arc::new(Mutex::new(
            settings_window::SettingsUi::new(&config, rt.clone(), tx.clone())
        ));
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
        match &self.weather {
            None => {
                tray.set_tooltip("Nimbus — завантаження…");
                tray.set_title("");
            }
            Some(w) => {
                let temp = match self.config.tray_temp {
                    TrayTemp::FeelsLike => w.current.feels_like,
                    TrayTemp::Actual    => w.current.temperature,
                };
                let (temp_str, unit) = match self.config.temp_unit {
                    config::TempUnit::Celsius    => (temp, "°C"),
                    config::TempUnit::Fahrenheit => (temp * 9.0 / 5.0 + 32.0, "°F"),
                };
                let sign = if temp_str >= 0.0 { "+" } else { "" };
                let title = format!("{}{:.0}{}", sign, temp_str, unit);

                let feels = match self.config.temp_unit {
                    config::TempUnit::Celsius    => w.current.feels_like,
                    config::TempUnit::Fahrenheit => w.current.feels_like * 9.0 / 5.0 + 32.0,
                };
                let tooltip = format!(
                    "Nimbus — {}\n{} / відчувається {}{:.0}{}\n{}",
                    self.config.city_name,
                    title,
                    if feels >= 0.0 { "+" } else { "" },
                    feels,
                    unit,
                    wmo_description_uk(w.current.weather_code),
                );
                tray.set_title(&title);
                tray.set_tooltip(&tooltip);
            }
        }
    }
}

impl eframe::App for NimbusApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if self.tray.is_none() {
            if let Ok(t) = Tray::build() {
                self.tray = Some(t);
            }
        }

        while let Ok(ev) = self.event_rx.try_recv() {
            match ev {
                AppEvent::WeatherReady(data) => {
                    self.fetching = false;
                    {
                        let mut ui = self.forecast_ui.lock().unwrap();
                        ui.weather = Some(*data.clone());
                        ui.city_name = self.config.city_name.clone();
                        ui.temp_unit = self.config.temp_unit;
                    }
                    self.weather = Some(*data);
                    self.update_tray();
                }
                AppEvent::WeatherError(e) => {
                    self.fetching = false;
                    log::error!("Помилка отримання погоди: {e}");
                }
                AppEvent::ConfigSaved(cfg) => {
                    self.config = cfg.clone();
                    self.settings_ui.lock().unwrap().sync_from_config(&cfg);
                    self.show_settings = false;
                    self.fetching = false;
                    self.spawn_fetch(ctx);
                }
            }
        }

        {
            let mut open_forecast = false;
            let mut open_settings = false;
            let mut do_refresh = false;
            let mut do_quit = false;

            if let Some(tray) = &self.tray {
                let (id_s, id_r, id_q) = (
                    tray.id_settings.clone(),
                    tray.id_refresh.clone(),
                    tray.id_quit.clone(),
                );
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
            if do_refresh    { self.spawn_fetch(ctx); }
            if do_quit       { ctx.send_viewport_cmd(egui::ViewportCommand::Close); }
        }

        let interval = Duration::from_secs(self.config.refresh_interval.minutes() * 60);
        if self.refresh_timer.elapsed() >= interval {
            self.spawn_fetch(ctx);
        }

        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
        ctx.request_repaint_after(Duration::from_secs(1));

        // Forecast
        if self.show_forecast {
            let ui = Arc::clone(&self.forecast_ui);
            let closed = Arc::new(Mutex::new(false));
            let closed2 = Arc::clone(&closed);
            ctx.show_viewport_immediate(
                egui::ViewportId::from_hash_of("forecast"),
                egui::ViewportBuilder::default()
                    .with_title("Nimbus")
                    .with_inner_size([580.0, 340.0])
                    .with_resizable(true),
                move |ctx, _| {
                    ui.lock().unwrap().show(ctx);
                    if ctx.input(|i| i.viewport().close_requested()) {
                        *closed2.lock().unwrap() = true;
                    }
                },
            );
            if *closed.lock().unwrap() { self.show_forecast = false; }
        }

        // Settings
        if self.show_settings {
            let ui = Arc::clone(&self.settings_ui);
            let closed = Arc::new(Mutex::new(false));
            let closed2 = Arc::clone(&closed);
            ctx.show_viewport_immediate(
                egui::ViewportId::from_hash_of("settings"),
                egui::ViewportBuilder::default()
                    .with_title("Nimbus — Налаштування")
                    .with_inner_size([380.0, 340.0])
                    .with_resizable(false),
                move |ctx, _| {
                    ui.lock().unwrap().show(ctx);
                    if ctx.input(|i| i.viewport().close_requested()) {
                        *closed2.lock().unwrap() = true;
                    }
                },
            );
            if *closed.lock().unwrap() { self.show_settings = false; }
        }
    }
}

/// WMO код → опис українською
pub fn wmo_description_uk(code: u8) -> &'static str {
    match code {
        0       => "Ясно",
        1       => "Переважно ясно",
        2       => "Мінлива хмарність",
        3       => "Хмарно",
        45 | 48 => "Туман",
        51 | 53 | 55 => "Мряка",
        61 | 63 | 65 => "Дощ",
        66 | 67 => "Крижаний дощ",
        71 | 73 | 75 => "Сніг",
        77      => "Снігові зерна",
        80 | 81 | 82 => "Зливи",
        85 | 86 => "Снігові зливи",
        95      => "Гроза",
        96 | 99 => "Гроза з градом",
        _       => "—",
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
                temp_unit: config::TempUnit::default(),
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

    let _ = eframe::run_native("Nimbus", opts, Box::new(|_cc| Ok(Box::new(app))));
    Ok(())
}
