use eframe::egui;
use crate::config::TempUnit;
use crate::main::wmo_description_uk;
use crate::weather::{WeatherData, wmo_icon, wind_direction_label};

#[derive(Clone, Default)]
pub struct ForecastUi {
    pub weather: Option<WeatherData>,
    pub city_name: String,
    pub temp_unit: TempUnit,
}

impl ForecastUi {
    pub fn new() -> Self { Self::default() }

    fn fmt_temp(&self, c: f32) -> String {
        match self.temp_unit {
            TempUnit::Celsius    => format!("{:.0}°C", c),
            TempUnit::Fahrenheit => format!("{:.0}°F", c * 9.0 / 5.0 + 32.0),
        }
    }

    pub fn show(&self, ctx: &egui::Context) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("Nimbus").size(18.0).strong());
                if !self.city_name.is_empty() {
                    ui.label(egui::RichText::new(format!("— {}", self.city_name))
                        .size(18.0).color(egui::Color32::GRAY));
                }
            });
            ui.separator();
            ui.add_space(4.0);

            let Some(data) = &self.weather else {
                ui.spinner(); return;
            };

            let cur = &data.current;
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(wmo_icon(cur.weather_code)).size(36.0));
                ui.add_space(8.0);
                ui.vertical(|ui| {
                    ui.label(egui::RichText::new(format!(
                        "{}  (відчувається {})",
                        self.fmt_temp(cur.temperature),
                        self.fmt_temp(cur.feels_like),
                    )).size(20.0).strong());
                    ui.label(egui::RichText::new(format!(
                        "{}   💨 {:.0} км/г {}   💧 {:.1} мм",
                        wmo_description_uk(cur.weather_code),
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
                        for h in &["День","","Стан","Темп","Відч.","Опади","Вітер","Напр."] {
                            ui.label(egui::RichText::new(*h).strong().size(11.0));
                        }
                        ui.end_row();
                        for day in &data.forecast {
                            ui.label(egui::RichText::new(day_label_uk(day.date.weekday())).size(12.0));
                            ui.label(egui::RichText::new(wmo_icon(day.weather_code)).size(14.0));
                            ui.label(egui::RichText::new(wmo_description_uk(day.weather_code)).size(12.0));
                            ui.label(egui::RichText::new(format!("{} / {}",
                                self.fmt_temp(day.temp_max), self.fmt_temp(day.temp_min))).size(12.0));
                            ui.label(egui::RichText::new(format!("{} / {}",
                                self.fmt_temp(day.feels_max), self.fmt_temp(day.feels_min))).size(12.0));
                            ui.label(egui::RichText::new(format!("{:.1}мм", day.precipitation)).size(12.0));
                            ui.label(egui::RichText::new(format!("{:.0}км/г", day.wind_speed)).size(12.0));
                            ui.label(egui::RichText::new(wind_direction_label(day.wind_direction)).size(12.0));
                            ui.end_row();
                        }
                    });
            });
        });
    }
}

fn day_label_uk(wd: chrono::Weekday) -> &'static str {
    match wd {
        chrono::Weekday::Mon => "Пн",
        chrono::Weekday::Tue => "Вт",
        chrono::Weekday::Wed => "Ср",
        chrono::Weekday::Thu => "Чт",
        chrono::Weekday::Fri => "Пт",
        chrono::Weekday::Sat => "Сб",
        chrono::Weekday::Sun => "Нд",
    }
}
