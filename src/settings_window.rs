use crate::config::{Config, RefreshInterval, TrayTemp};
use crate::geo::{search_cities, GeoResult};
use egui::{self, Color32, CentralPanel, RichText, Ui};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

#[derive(Default)]
struct SearchState {
    results: Vec<GeoResult>,
    loading: bool,
    error: Option<String>,
}

pub struct SettingsWindow {
    tray_temp: TrayTemp,
    refresh_interval: RefreshInterval,

    city_query: String,
    last_keystroke: Option<Instant>,
    search_state: Arc<Mutex<SearchState>>,
    selected_city: Option<GeoResult>,

    /// Set when the user clicks Save; caller must consume this
    pub pending_config: Option<Config>,
}

impl SettingsWindow {
    pub fn new(config: &Config) -> Self {
        Self {
            tray_temp: config.tray_temp,
            refresh_interval: config.refresh_interval,
            city_query: config.city_name.clone(),
            last_keystroke: None,
            search_state: Arc::new(Mutex::new(SearchState::default())),
            selected_city: None,
            pending_config: None,
        }
    }

    pub fn sync_from_config(&mut self, config: &Config) {
        self.tray_temp = config.tray_temp;
        self.refresh_interval = config.refresh_interval;
        self.city_query = config.city_name.clone();
        self.selected_city = None;
        if let Ok(mut s) = self.search_state.lock() {
            s.results.clear();
            s.error = None;
        }
    }

    pub fn show(&mut self, ctx: &egui::Context, rt: &tokio::runtime::Handle) {
        // Fire debounced search
        if let Some(t) = self.last_keystroke {
            if t.elapsed() >= Duration::from_millis(300) {
                self.last_keystroke = None;
                self.trigger_search(rt, ctx);
            } else {
                ctx.request_repaint_after(Duration::from_millis(300));
            }
        }

        CentralPanel::default().show(ctx, |ui| {
            ui.add_space(8.0);
            self.draw_city(ui, rt, ctx);
            ui.add_space(12.0);
            ui.separator();
            ui.add_space(8.0);
            self.draw_display(ui);
            ui.add_space(12.0);
            ui.separator();
            ui.add_space(8.0);
            self.draw_refresh(ui);
            ui.add_space(16.0);
            self.draw_buttons(ui);
        });
    }

    fn draw_city(&mut self, ui: &mut Ui, rt: &tokio::runtime::Handle, ctx: &egui::Context) {
        ui.label(RichText::new("City").strong());
        ui.add_space(4.0);

        let resp = ui.add(
            egui::TextEdit::singleline(&mut self.city_query)
                .hint_text("Search city…")
                .desired_width(320.0),
        );

        if resp.changed() {
            self.last_keystroke = Some(Instant::now());
            self.selected_city = None;
            if let Ok(mut s) = self.search_state.lock() {
                s.results.clear();
                s.error = None;
            }
        }

        let state = self.search_state.lock().unwrap();
        if state.loading {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(RichText::new("Searching…").color(Color32::GRAY).size(12.0));
            });
        } else if let Some(err) = &state.error {
            ui.label(RichText::new(err.as_str()).color(Color32::RED).size(12.0));
        } else if !state.results.is_empty() {
            let results = state.results.clone();
            drop(state);

            egui::Frame::none()
                .fill(ui.visuals().extreme_bg_color)
                .rounding(4.0)
                .inner_margin(4.0)
                .show(ui, |ui| {
                    egui::ScrollArea::vertical()
                        .max_height(150.0)
                        .show(ui, |ui| {
                            for city in &results {
                                let label = city.display_label();
                                let selected = self
                                    .selected_city
                                    .as_ref()
                                    .map(|c| {
                                        c.name == city.name && c.latitude == city.latitude
                                    })
                                    .unwrap_or(false);

                                if ui.selectable_label(selected, &label).clicked() {
                                    self.selected_city = Some(city.clone());
                                    self.city_query = city.name.clone();
                                    if let Ok(mut s) = self.search_state.lock() {
                                        s.results.clear();
                                    }
                                }
                            }
                        });
                });
        }
    }

    fn draw_display(&mut self, ui: &mut Ui) {
        ui.label(RichText::new("Show in tray").strong());
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.radio_value(&mut self.tray_temp, TrayTemp::Actual, "Actual temperature");
            ui.radio_value(&mut self.tray_temp, TrayTemp::FeelsLike, "Feels like");
        });
    }

    fn draw_refresh(&mut self, ui: &mut Ui) {
        ui.label(RichText::new("Refresh interval").strong());
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            for &interval in RefreshInterval::all() {
                ui.radio_value(&mut self.refresh_interval, interval, interval.label());
            }
        });
    }

    fn draw_buttons(&mut self, ui: &mut Ui) {
        let can_save = self.selected_city.is_some();

        ui.horizontal(|ui| {
            if ui
                .add_enabled(can_save, egui::Button::new("Save"))
                .clicked()
            {
                if let Some(city) = &self.selected_city {
                    self.pending_config = Some(Config {
                        city_name: city.name.clone(),
                        latitude: city.latitude,
                        longitude: city.longitude,
                        tray_temp: self.tray_temp,
                        refresh_interval: self.refresh_interval,
                    });
                }
            }
            if ui.button("Cancel").clicked() {
                // Caller hides the window; we just clear state
                self.selected_city = None;
                if let Ok(mut s) = self.search_state.lock() {
                    s.results.clear();
                }
            }
        });

        if !can_save && !self.city_query.is_empty() {
            ui.add_space(4.0);
            ui.label(
                RichText::new("Select a city from the search results")
                    .color(Color32::GRAY)
                    .size(11.0),
            );
        }
    }

    fn trigger_search(&mut self, rt: &tokio::runtime::Handle, ctx: &egui::Context) {
        let query = self.city_query.trim().to_string();
        if query.is_empty() {
            return;
        }

        {
            let mut s = self.search_state.lock().unwrap();
            s.loading = true;
            s.results.clear();
            s.error = None;
        }

        let state = Arc::clone(&self.search_state);
        let ctx = ctx.clone();

        rt.spawn(async move {
            match search_cities(&query, 8).await {
                Ok(results) => {
                    let mut s = state.lock().unwrap();
                    s.results = results;
                    s.loading = false;
                }
                Err(e) => {
                    let mut s = state.lock().unwrap();
                    s.error = Some(format!("Search failed: {e}"));
                    s.loading = false;
                }
            }
            ctx.request_repaint();
        });
    }
}
