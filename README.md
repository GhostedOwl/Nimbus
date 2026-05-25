# Nimbus 🌤

A minimal, cross-platform weather tray app written in Rust.

## Features

- **System tray** with current temperature (actual or feels like)
- **7-day forecast** popup on left click
- **City search** with debounced autocomplete (Open-Meteo Geocoding)
- **Auto-detect location** on first run via ip-api.com
- **Configurable** refresh interval (15 / 30 / 60 min)
- No WebView, no Electron — native egui UI

## Config

Stored at `~/.config/nimbus/config.toml`:

```toml
city_name = "Kyiv"
latitude = 50.45
longitude = 30.52
tray_temp = "actual"      # or "feels_like"
refresh_interval = "30"   # "15", "30", or "60"
```

## Build

```bash
# Debug
cargo build

# Release (smaller binary, LTO enabled)
cargo build --release
```

### Linux dependencies

On Linux, `tray-icon` requires one of:
- **X11**: `libxdo-dev` (for `xdotool`-based positioning)
- **Wayland/AppIndicator**: `libappindicator3-dev`

```bash
# Debian/Ubuntu
sudo apt install libxdo-dev   # X11
# or
sudo apt install libappindicator3-dev   # GNOME/AppIndicator
```

### Windows / macOS

No extra system dependencies — just `cargo build --release`.

## Architecture

```
main.rs              — winit event loop + tray setup + tokio runtime
config.rs            — TOML config load/save
geo.rs               — IP geolocation + Open-Meteo Geocoding API
weather.rs           — Open-Meteo forecast API
forecast_window.rs   — egui 7-day forecast popup
settings_window.rs   — egui settings with city search
```

## APIs used

| API | Purpose | Auth |
|-----|---------|------|
| [Open-Meteo](https://open-meteo.com) | Weather forecast | Free, no key |
| [Open-Meteo Geocoding](https://open-meteo.com/en/docs/geocoding-api) | City search | Free, no key |
| [ip-api.com](http://ip-api.com) | First-run location | Free, no key |

## Roadmap

- [ ] Proper weather icon set (SVG)
- [ ] Unit preference (°C / °F)
- [ ] Multiple locations
- [ ] Desktop notifications for rain/severe weather
- [ ] Auto-start on login
