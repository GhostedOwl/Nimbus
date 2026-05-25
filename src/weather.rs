use anyhow::{Context, Result};
use chrono::NaiveDate;
use serde::Deserialize;

/// WMO weather code → short description + emoji-free icon char
pub fn wmo_description(code: u8) -> &'static str {
    match code {
        0 => "Clear sky",
        1 => "Mainly clear",
        2 => "Partly cloudy",
        3 => "Overcast",
        45 | 48 => "Fog",
        51 | 53 | 55 => "Drizzle",
        61 | 63 | 65 => "Rain",
        71 | 73 | 75 => "Snow",
        77 => "Snow grains",
        80 | 81 | 82 => "Rain showers",
        85 | 86 => "Snow showers",
        95 => "Thunderstorm",
        96 | 99 => "Thunderstorm w/ hail",
        _ => "Unknown",
    }
}

/// Very simple weather icon using ASCII-safe chars (for tray tooltip)
pub fn wmo_icon(code: u8) -> &'static str {
    match code {
        0 => "☀",
        1 | 2 => "⛅",
        3 => "☁",
        45 | 48 => "🌫",
        51..=55 | 80..=82 => "🌦",
        61..=65 => "🌧",
        71..=77 | 85 | 86 => "❄",
        95..=99 => "⛈",
        _ => "?",
    }
}

/// Wind direction degrees → compass abbreviation
pub fn wind_direction_label(degrees: f32) -> &'static str {
    let idx = ((degrees + 22.5) / 45.0) as usize % 8;
    ["N", "NE", "E", "SE", "S", "SW", "W", "NW"][idx]
}

// ─── Raw API response types ───────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct RawCurrentWeather {
    pub temperature_2m: f32,
    pub apparent_temperature: f32,
    pub precipitation: f32,
    pub wind_speed_10m: f32,
    pub wind_direction_10m: f32,
    pub weather_code: u8,
}

#[derive(Debug, Deserialize)]
pub struct RawDailyWeather {
    pub time: Vec<String>,             // "2025-05-01"
    pub temperature_2m_max: Vec<f32>,
    pub temperature_2m_min: Vec<f32>,
    pub apparent_temperature_max: Vec<f32>,
    pub apparent_temperature_min: Vec<f32>,
    pub precipitation_sum: Vec<f32>,
    pub wind_speed_10m_max: Vec<f32>,
    pub wind_direction_10m_dominant: Vec<f32>,
    pub weather_code: Vec<u8>,
}

#[derive(Debug, Deserialize)]
pub struct RawWeatherResponse {
    pub current: RawCurrentWeather,
    pub daily: RawDailyWeather,
}

// ─── Processed types ──────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct CurrentWeather {
    pub temperature: f32,
    pub feels_like: f32,
    pub precipitation: f32,
    pub wind_speed: f32,
    pub wind_direction: f32,
    pub weather_code: u8,
}

impl CurrentWeather {
    pub fn description(&self) -> &'static str {
        wmo_description(self.weather_code)
    }
    pub fn icon(&self) -> &'static str {
        wmo_icon(self.weather_code)
    }
    pub fn wind_dir_label(&self) -> &'static str {
        wind_direction_label(self.wind_direction)
    }
}

#[derive(Debug, Clone)]
pub struct DayForecast {
    pub date: NaiveDate,
    pub temp_max: f32,
    pub temp_min: f32,
    pub feels_max: f32,
    pub feels_min: f32,
    pub precipitation: f32,
    pub wind_speed: f32,
    pub wind_direction: f32,
    pub weather_code: u8,
}

impl DayForecast {
    pub fn description(&self) -> &'static str {
        wmo_description(self.weather_code)
    }
    pub fn icon(&self) -> &'static str {
        wmo_icon(self.weather_code)
    }
    pub fn wind_dir_label(&self) -> &'static str {
        wind_direction_label(self.wind_direction)
    }
}

#[derive(Debug, Clone)]
pub struct WeatherData {
    pub current: CurrentWeather,
    pub forecast: Vec<DayForecast>, // 7 days
}

// ─── Fetcher ──────────────────────────────────────────────────────────────────

pub async fn fetch_weather(lat: f64, lon: f64) -> Result<WeatherData> {
    let url = format!(
        "https://api.open-meteo.com/v1/forecast\
         ?latitude={lat}&longitude={lon}\
         &current=temperature_2m,apparent_temperature,precipitation,\
                  wind_speed_10m,wind_direction_10m,weather_code\
         &daily=weather_code,temperature_2m_max,temperature_2m_min,\
                apparent_temperature_max,apparent_temperature_min,\
                precipitation_sum,wind_speed_10m_max,\
                wind_direction_10m_dominant\
         &wind_speed_unit=kmh\
         &forecast_days=7\
         &format=json"
    );

    log::info!("Fetching weather for ({lat}, {lon})");
    let resp = reqwest::get(&url)
        .await
        .context("Failed to reach Open-Meteo API")?;

    if !resp.status().is_success() {
        anyhow::bail!("Open-Meteo returned HTTP {}", resp.status());
    }

    let raw: RawWeatherResponse = resp
        .json()
        .await
        .context("Failed to parse Open-Meteo response")?;

    let current = CurrentWeather {
        temperature: raw.current.temperature_2m,
        feels_like: raw.current.apparent_temperature,
        precipitation: raw.current.precipitation,
        wind_speed: raw.current.wind_speed_10m,
        wind_direction: raw.current.wind_direction_10m,
        weather_code: raw.current.weather_code,
    };

    let daily = &raw.daily;
    let n = daily.time.len().min(7);
    let mut forecast = Vec::with_capacity(n);

    for i in 0..n {
        let date = daily.time[i]
            .parse::<NaiveDate>()
            .with_context(|| format!("Bad date: {}", daily.time[i]))?;

        forecast.push(DayForecast {
            date,
            temp_max: daily.temperature_2m_max[i],
            temp_min: daily.temperature_2m_min[i],
            feels_max: daily.apparent_temperature_max[i],
            feels_min: daily.apparent_temperature_min[i],
            precipitation: daily.precipitation_sum[i],
            wind_speed: daily.wind_speed_10m_max[i],
            wind_direction: daily.wind_direction_10m_dominant[i],
            weather_code: daily.weather_code[i],
        });
    }

    Ok(WeatherData { current, forecast })
}
