use eframe::egui;
use crate::config::{Config, RefreshInterval, TempUnit, TrayTemp};
use crate::geo::{search_cities, GeoResult};
use crate::AppEvent;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

#[derive(Default)]
struct SearchState {
    results: Vec<GeoResult>,
    loading: bool,
    error: Option<String>,
}

#[derive(Clone)]
pub struct SettingsUi {
    pub tray_temp: TrayTemp,
    pub refresh_interval: RefreshInterval,
    pub temp_unit: TempUnit,
    query: String,
    last_key: Option<Instant>,
    search: Arc<Mutex<SearchState>>,
    selected: Option<GeoResult>,
    tx: std::sync::mpsc::Sender<AppEvent>,
    rt: tokio::runtime::Handle,
    status: String,
}

impl SettingsUi {
    pub fn new(config: &Config, rt: tokio::runtime::Handle, tx: std::sync::mpsc::Sender<AppEvent>) -> Self {
        Self {
            tray_temp: config.tray_temp,
            refresh_interval: config.refresh_interval,
            temp_unit: config.temp_unit,
            query: config.city_name.clone(),
            last_key: None,
            search: Arc::new(Mutex::new(SearchState::default())),
            selected: None,
            tx, rt,
            status: String::new(),
        }
    }

    pub fn sync_from_config(&mut self, config: &Config) {
        self.tray_temp = config.tray_temp;
        self.refresh_interval = config.refresh_interval;
        self.temp_unit = config.temp_unit;
        self.query = config.city_name.clone();
        self.selected = None;
        if let Ok(mut s) = self.search.lock() { s.results.clear(); s.error = None; }
    }

    pub fn show(&mut self, ctx: &egui::Context) {
        if let Some(t) = self.last_key {
            if t.elapsed() >= Duration::from_millis(300) {
                self.last_key = None;
                let q = self.query.trim().to_string();
                if !q.is_empty() {
                    let state = Arc::clone(&self.search);
                    let ctx2 = ctx.clone();
                    { let mut s = state.lock().unwrap(); s.loading = true; s.results.clear(); s.error = None; }
                    self.rt.spawn(async move {
                        match search_cities(&q, 8).await {
                            Ok(r)  => { let mut s = state.lock().unwrap(); s.results = r; s.loading = false; }
                            Err(e) => { let mut s = state.lock().unwrap(); s.error = Some(e.to_string()); s.loading = false; }
                        }
                        ctx2.request_repaint();
                    });
                }
            } else {
                ctx.request_repaint_after(Duration::from_millis(100));
            }
        }

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.add_space(8.0);

            // City search
            ui.label(egui::RichText::new("Місто").strong());
            ui.add_space(4.0);
            let resp = ui.add(egui::TextEdit::singleline(&mut self.query)
                .hint_text("Почніть вводити назву…").desired_width(320.0));
            if resp.changed() {
                self.last_key = Some(Instant::now());
                self.selected = None;
                if let Ok(mut s) = self.search.lock() { s.results.clear(); }
            }

            {
                let state = self.search.lock().unwrap();
                if state.loading {
                    ui.horizontal(|ui| { ui.spinner(); ui.label("Пошук…"); });
                } else if let Some(err) = &state.error {
                    ui.colored_label(egui::Color32::RED, err.as_str());
                }
            }

            let results = self.search.lock().unwrap().results.clone();
            if !results.is_empty() {
                egui::ScrollArea::vertical().max_height(130.0).id_salt("city_results").show(ui, |ui| {
                    for city in &results {
                        let sel = self.selected.as_ref()
                            .map(|c| c.latitude == city.latitude && c.longitude == city.longitude)
                            .unwrap_or(false);
                        if ui.selectable_label(sel, &city.display_label()).clicked() {
                            self.query = city.name.clone();
                            self.selected = Some(city.clone());
                            if let Ok(mut s) = self.search.lock() { s.results.clear(); }
                        }
                    }
                });
            }

            ui.add_space(8.0);
            ui.separator();
            ui.add_space(6.0);

            // Tray display
            ui.label(egui::RichText::new("Показувати в треї").strong());
            ui.horizontal(|ui| {
                ui.radio_value(&mut self.tray_temp, TrayTemp::Actual,    "Фактична");
                ui.radio_value(&mut self.tray_temp, TrayTemp::FeelsLike, "Відчувається");
            });

            ui.add_space(6.0);

            // Temp unit
            ui.label(egui::RichText::new("Одиниці температури").strong());
            ui.horizontal(|ui| {
                ui.radio_value(&mut self.temp_unit, TempUnit::Celsius,    "°C");
                ui.radio_value(&mut self.temp_unit, TempUnit::Fahrenheit, "°F");
            });

            ui.add_space(6.0);

            // Refresh
            ui.label(egui::RichText::new("Оновлення").strong());
            ui.horizontal(|ui| {
                for &iv in RefreshInterval::all() {
                    ui.radio_value(&mut self.refresh_interval, iv, iv.label());
                }
            });

            ui.add_space(10.0);
            ui.separator();
            ui.add_space(4.0);

            if !self.status.is_empty() {
                ui.colored_label(egui::Color32::from_rgb(80, 180, 80), &self.status);
                ui.add_space(4.0);
            }

            let can_save = self.selected.is_some();
            ui.horizontal(|ui| {
                if ui.add_enabled(can_save, egui::Button::new("Зберегти")).clicked() {
                    if let Some(city) = &self.selected {
                        let new_cfg = Config {
                            city_name: city.name.clone(),
                            latitude: city.latitude,
                            longitude: city.longitude,
                            tray_temp: self.tray_temp,
                            refresh_interval: self.refresh_interval,
                            temp_unit: self.temp_unit,
                        };
                        match new_cfg.save() {
                            Ok(_)  => { let _ = self.tx.send(AppEvent::ConfigSaved(new_cfg)); }
                            Err(e) => { self.status = format!("Помилка: {e}"); }
                        }
                    }
                }
                if ui.button("Скасувати").clicked() {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            });
            if !can_save && !self.query.is_empty() {
                ui.label(egui::RichText::new("Оберіть місто зі списку")
                    .color(egui::Color32::GRAY).size(11.0));
            }
        });
    }
}
