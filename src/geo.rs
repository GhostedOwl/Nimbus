use anyhow::{Context, Result};
use serde::Deserialize;

/// Result from ip-api.com (used for first-run auto-detect)
#[derive(Debug, Deserialize)]
pub struct IpLocation {
    pub city: String,
    pub lat: f64,
    pub lon: f64,
    pub status: String,
}

/// Detect city/coordinates from current IP address
pub async fn detect_location() -> Result<IpLocation> {
    let url = "http://ip-api.com/json/?fields=status,city,lat,lon";
    let resp = reqwest::get(url)
        .await
        .context("Failed to reach ip-api.com")?;
    let loc: IpLocation = resp
        .json()
        .await
        .context("Failed to parse ip-api.com response")?;
    if loc.status != "success" {
        anyhow::bail!("ip-api.com returned non-success status");
    }
    log::info!("Auto-detected location: {} ({}, {})", loc.city, loc.lat, loc.lon);
    Ok(loc)
}

/// A city search result from Open-Meteo Geocoding API
#[derive(Debug, Clone, Deserialize)]
pub struct GeoResult {
    pub name: String,
    pub latitude: f64,
    pub longitude: f64,
    /// Country name (optional in API response)
    #[serde(default)]
    pub country: String,
    /// Admin region (state/province, optional)
    #[serde(default)]
    pub admin1: Option<String>,
}

impl GeoResult {
    /// Human-readable label: "Kyiv, Ukraine" or "Kyiv, Kyiv Oblast, Ukraine"
    pub fn display_label(&self) -> String {
        match &self.admin1 {
            Some(region) if !region.is_empty() => {
                format!("{}, {}, {}", self.name, region, self.country)
            }
            _ => format!("{}, {}", self.name, self.country),
        }
    }
}

#[derive(Debug, Deserialize)]
struct GeoResponse {
    #[serde(default)]
    results: Vec<GeoResult>,
}

/// Search for cities matching a query string.
/// Returns up to `count` results (max 10 by API).
pub async fn search_cities(query: &str, count: u8) -> Result<Vec<GeoResult>> {
    if query.trim().is_empty() {
        return Ok(vec![]);
    }
    let url = format!(
        "https://geocoding-api.open-meteo.com/v1/search?name={}&count={}&language=en&format=json",
        urlencoding::encode(query),
        count
    );
    let resp = reqwest::get(&url)
        .await
        .context("Failed to reach Open-Meteo Geocoding API")?;
    let geo: GeoResponse = resp
        .json()
        .await
        .context("Failed to parse Geocoding API response")?;
    Ok(geo.results)
}
