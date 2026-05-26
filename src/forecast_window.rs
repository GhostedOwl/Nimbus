use eframe::egui;
use crate::weather::{WeatherData, wmo_description, wmo_icon, wind_direction_label};

pub struct ForecastApp {
    pub data: WeatherData,
    pub city_name: String,
}

impl eframe::App for ForecastApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("Nimbus").size(18.0).strong());
                ui.label(egui::RichText::new(format!("— {}", self.city_name))
                    .size(18.0).color(egui::Color32::GRAY));
            });
            ui.separator();
            ui.add_space(4.0);

            let cur = &self.data.current;
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(wmo_icon(cur.weather_code)).size(36.0));
                ui.add_space(8.0);
                ui.vertical(|ui| {
                    ui.label(egui::RichText::new(format!(
                        "{:.1}°C  (feels {:.1}°C)", cur.temperature, cur.feels_like
                    )).size(22.0).strong());
                    ui.label(egui::RichText::new(format!(
                        "{}   💨 {:.0} km/h {}   💧 {:.1} mm",
                        wmo_description(cur.weather_code),
                        cur.wind_speed,
                        wind_direction_label(cur.wind_direction),
                        cur.precipitation,
                    )).color(egui::Color32::GRAY));
                });
            });

            ui.add_space(8.0);
            ui.separator();
            ui.add_space(4.0);

            egui::ScrollArea::vertical().show(ui, |ui| {
                egui::Grid::new("forecast")
                    .num_columns(8).striped(true).spacing([10.0, 4.0])
                    .show(ui, |ui| {
                        for h in &["Day", "", "Condition", "Temp", "Feels", "Precip", "Wind", "Dir"] {
                            ui.label(egui::RichText::new(*h).strong().size(11.0));
                        }
                        ui.end_row();
                        for day in &self.data.forecast {
                            ui.label(egui::RichText::new(day.date.format("%a %m/%d").to_string()).size(12.0));
                            ui.label(egui::RichText::new(wmo_icon(day.weather_code)).size(14.0));
                            ui.label(egui::RichText::new(wmo_description(day.weather_code)).size(12.0));
                            ui.label(egui::RichText::new(format!("{:.0}°/{:.0}°", day.temp_max, day.temp_min)).size(12.0));
                            ui.label(egui::RichText::new(format!("{:.0}°/{:.0}°", day.feels_max, day.feels_min)).size(12.0));
                            ui.label(egui::RichText::new(format!("{:.1}mm", day.precipitation)).size(12.0));
                            ui.label(egui::RichText::new(format!("{:.0}km/h", day.wind_speed)).size(12.0));
                            ui.label(egui::RichText::new(wind_direction_label(day.wind_direction)).size(12.0));
                            ui.end_row();
                        }
                    });
            });
        });
    }
}
