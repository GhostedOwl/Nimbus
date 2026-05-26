use egui::{self, CentralPanel, Color32, Grid, RichText, ScrollArea};
use crate::weather::{WeatherData, wmo_description, wmo_icon, wind_direction_label};

pub struct ForecastWindow {
    pub weather: Option<WeatherData>,
    pub city_name: String,
}

impl ForecastWindow {
    pub fn new() -> Self {
        Self {
            weather: None,
            city_name: String::new(),
        }
    }

    pub fn show(&self, ctx: &egui::Context) {
        CentralPanel::default().show(ctx, |ui| {
            // Header
            ui.horizontal(|ui| {
                ui.label(RichText::new("Nimbus").size(18.0).strong());
                if !self.city_name.is_empty() {
                    ui.label(
                        RichText::new(format!("— {}", self.city_name))
                            .size(18.0)
                            .color(Color32::GRAY),
                    );
                }
            });
            ui.separator();
            ui.add_space(4.0);

            match &self.weather {
                None => {
                    ui.centered_and_justified(|ui| {
                        ui.spinner();
                    });
                }
                Some(data) => {
                    self.draw_current(ui, data);
                    ui.add_space(8.0);
                    ui.separator();
                    ui.add_space(4.0);
                    self.draw_forecast(ui, data);
                }
            }
        });
    }

    fn draw_current(&self, ui: &mut egui::Ui, data: &WeatherData) {
        let cur = &data.current;
        ui.horizontal(|ui| {
            ui.label(RichText::new(wmo_icon(cur.weather_code)).size(36.0));
            ui.add_space(8.0);
            ui.vertical(|ui| {
                ui.label(
                    RichText::new(format!(
                        "{:.1}°C  (feels {:.1}°C)",
                        cur.temperature, cur.feels_like
                    ))
                    .size(22.0)
                    .strong(),
                );
                ui.label(
                    RichText::new(format!(
                        "{}   💨 {:.0} km/h {}   💧 {:.1} mm",
                        wmo_description(cur.weather_code),
                        cur.wind_speed,
                        wind_direction_label(cur.wind_direction),
                        cur.precipitation,
                    ))
                    .color(Color32::GRAY),
                );
            });
        });
    }

    fn draw_forecast(&self, ui: &mut egui::Ui, data: &WeatherData) {
        ScrollArea::vertical().show(ui, |ui| {
            Grid::new("forecast_grid")
                .num_columns(8)
                .striped(true)
                .spacing([10.0, 5.0])
                .show(ui, |ui| {
                    // Column headers
                    for h in &["Day", "", "Condition", "Temp", "Feels", "Precip", "Wind", "Dir"] {
                        ui.label(RichText::new(*h).strong().size(11.0));
                    }
                    ui.end_row();

                    for day in &data.forecast {
                        // Day label: "Mon 05/26"
                        let weekday = day.date.format("%a %m/%d").to_string();
                        ui.label(RichText::new(weekday).size(12.0));
                        ui.label(RichText::new(wmo_icon(day.weather_code)).size(16.0));
                        ui.label(RichText::new(wmo_description(day.weather_code)).size(12.0));
                        ui.label(
                            RichText::new(format!("{:.0}° / {:.0}°", day.temp_max, day.temp_min))
                                .size(12.0),
                        );
                        ui.label(
                            RichText::new(format!(
                                "{:.0}° / {:.0}°",
                                day.feels_max, day.feels_min
                            ))
                            .size(12.0),
                        );
                        ui.label(
                            RichText::new(format!("{:.1} mm", day.precipitation))
                                .size(12.0),
                        );
                        ui.label(
                            RichText::new(format!("{:.0} km/h", day.wind_speed))
                                .size(12.0),
                        );
                        ui.label(
                            RichText::new(wind_direction_label(day.wind_direction))
                                .size(12.0),
                        );
                        ui.end_row();
                    }
                });
        });
    }
}
