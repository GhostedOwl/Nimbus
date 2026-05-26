use anyhow::Result;
use tray_icon::{
    menu::{Menu, MenuEvent, MenuItem, MenuId, PredefinedMenuItem},
    TrayIcon, TrayIconBuilder, Icon,
};

pub struct Tray {
    _icon: TrayIcon,
    pub id_settings: MenuId,
    pub id_refresh: MenuId,
    pub id_quit: MenuId,
}

impl Tray {
    pub fn build() -> Result<Self> {
        let item_settings = MenuItem::new("Settings", true, None);
        let item_refresh  = MenuItem::new("Refresh",  true, None);
        let item_quit     = MenuItem::new("Quit",     true, None);

        let id_settings = item_settings.id().clone();
        let id_refresh  = item_refresh.id().clone();
        let id_quit     = item_quit.id().clone();

        let menu = Menu::new();
        menu.append(&item_settings)?;
        menu.append(&item_refresh)?;
        menu.append(&PredefinedMenuItem::separator())?;
        menu.append(&item_quit)?;

        let tray = TrayIconBuilder::new()
            .with_menu(Box::new(menu))
            .with_tooltip("Nimbus")
            .with_icon(icon_for_code(0))
            .build()?;

        Ok(Self { _icon: tray, id_settings, id_refresh, id_quit })
    }

    pub fn set_tooltip(&self, text: &str) {
        if let Err(e) = self._icon.set_tooltip(Some(text)) {
            log::warn!("set_tooltip failed: {e}");
        }
    }

    pub fn set_weather_icon(&self, weather_code: u8) {
        let icon = icon_for_code(weather_code);
        if let Err(e) = self._icon.set_icon(Some(icon)) {
            log::warn!("set_icon failed: {e}");
        }
    }

    pub fn poll_menu(&self) -> Option<MenuEvent> {
        MenuEvent::receiver().try_recv().ok()
    }
}

/// Pick icon based on WMO weather code
pub fn icon_for_code(code: u8) -> Icon {
    match code {
        0 | 1              => load_icon(include_bytes!("../assets/icon_sun.png")),
        2 | 3 | 45 | 48    => load_icon(include_bytes!("../assets/icon_cloud.png")),
        51..=67 | 80..=82  => load_icon(include_bytes!("../assets/icon_rain.png")),
        71..=77 | 85 | 86  => load_icon(include_bytes!("../assets/icon_snow.png")),
        95..=99            => load_icon(include_bytes!("../assets/icon_rain.png")),
        _                  => load_icon(include_bytes!("../assets/icon_cloud.png")),
    }
}

fn load_icon(png_bytes: &[u8]) -> Icon {
    // Decode PNG to raw RGBA using the image crate (already a dep via eframe)
    use image::ImageDecoder;
    let cursor = std::io::Cursor::new(png_bytes);
    let decoder = image::codecs::png::PngDecoder::new(cursor).expect("png decode");
    let (w, h) = decoder.dimensions();
    let mut rgba = vec![0u8; (w * h * 4) as usize];
    decoder.read_image(&mut rgba).expect("png read");
    Icon::from_rgba(rgba, w, h).expect("tray icon")
}
