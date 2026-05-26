use eframe::egui;
use crate::config::{Config, RefreshInterval, TrayTemp};
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

pub struct SettingsApp {
    config: Config,
    query: String,
    last_key: Option<Instant>,
    search: Arc<Mutex<SearchState>>,
    selected: Option<GeoResult>,
    proxy: winit::event_loop::EventLoopProxy<AppEvent>,
    rt: tokio::runtime::Handle,
    status: String,
}

impl SettingsApp {
    pub fn new(
        config: Config,
        rt: tokio::runtime::Handle,
        proxy: winit::event_loop::EventLoopProxy<AppEvent>,
    ) -> Self {
        Self {
            query: config.city_name.clone(),
            config,
            last_key: None,
            search: Arc::new(Mutex::new(SearchState::default())),
            selected: None,
            proxy,
            rt,
            status: String::new(),
        }
    }
}

impl eframe::App for SettingsApp {
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        // Debounce search
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
                            Ok(r) => { let mut s = state.lock().unwrap(); s.results = r; s.loading = false; }
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
            ui.label(egui::RichText::new("City").strong());
            ui.add_space(4.0);

            let resp = ui.add(egui::TextEdit::singleline(&mut self.query)
                .hint_text("Search city…").desired_width(300.0));
            if resp.changed() {
                self.last_key = Some(Instant::now());
                self.selected = None;
                self.search.lock().unwrap().results.clear();
            }

            let state = self.search.lock().unwrap();
            if state.loading {
                ui.spinner();
            } else if let Some(err) = &state.error {
                ui.colored_label(egui::Color32::RED, err.as_str());
            } else if !state.results.is_empty() {
                let results = state.results.clone();
                drop(state);
                egui::ScrollArea::vertical().max_height(140.0).show(ui, |ui| {
                    for city in &results {
                        let label = city.display_label();
                        let sel = self.selected.as_ref()
                            .map(|c| c.latitude == city.latitude && c.longitude == city.longitude)
                            .unwrap_or(false);
                        if ui.selectable_label(sel, &label).clicked() {
                            self.query = city.name.clone();
                            self.selected = Some(city.clone());
                            self.search.lock().unwrap().results.clear();
                        }
                    }
                });
            }

            ui.add_space(10.0);
            ui.separator();
            ui.add_space(6.0);

            ui.label(egui::RichText::new("Show in tray").strong());
            ui.horizontal(|ui| {
                ui.radio_value(&mut self.config.tray_temp, TrayTemp::Actual, "Actual");
                ui.radio_value(&mut self.config.tray_temp, TrayTemp::FeelsLike, "Feels like");
            });

            ui.add_space(8.0);
            ui.label(egui::RichText::new("Refresh interval").strong());
            ui.horizontal(|ui| {
                for &iv in RefreshInterval::all() {
                    ui.radio_value(&mut self.config.refresh_interval, iv, iv.label());
                }
            });

            ui.add_space(12.0);
            ui.separator();

            if !self.status.is_empty() {
                ui.colored_label(egui::Color32::from_rgb(80, 180, 80), &self.status);
                ui.add_space(4.0);
            }

            let can_save = self.selected.is_some();
            ui.horizontal(|ui| {
                if ui.add_enabled(can_save, egui::Button::new("Save")).clicked() {
                    if let Some(city) = &self.selected {
                        let new_cfg = Config {
                            city_name: city.name.clone(),
                            latitude: city.latitude,
                            longitude: city.longitude,
                            tray_temp: self.config.tray_temp,
                            refresh_interval: self.config.refresh_interval,
                        };
                        if let Err(e) = new_cfg.save() {
                            self.status = format!("Save failed: {e}");
                        } else {
                            let _ = self.proxy.send_event(AppEvent::ConfigSaved(new_cfg));
                            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                    }
                }
                if ui.button("Cancel").clicked() {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            });
            if !can_save && !self.query.is_empty() {
                ui.label(egui::RichText::new("Select a city from results").color(egui::Color32::GRAY).size(11.0));
            }
        });
    }
}
