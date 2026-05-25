use anyhow::Result;
use tray_icon::{
    menu::{Menu, MenuEvent, MenuItem, MenuId, PredefinedMenuItem},
    TrayIcon, TrayIconBuilder,
};

/// Owns the tray icon and exposes menu item IDs for event matching
pub struct Tray {
    /// Keep alive — dropping this removes the tray icon
    _icon: TrayIcon,
    pub id_settings: MenuId,
    pub id_refresh: MenuId,
    pub id_quit: MenuId,
}

impl Tray {
    pub fn build() -> Result<Self> {
        let item_settings = MenuItem::new("Settings", true, None);
        let item_refresh = MenuItem::new("Refresh", true, None);
        let item_quit = MenuItem::new("Quit", true, None);

        let id_settings = item_settings.id().clone();
        let id_refresh = item_refresh.id().clone();
        let id_quit = item_quit.id().clone();

        let menu = Menu::new();
        menu.append(&item_settings)?;
        menu.append(&item_refresh)?;
        menu.append(&PredefinedMenuItem::separator())?;
        menu.append(&item_quit)?;

        let icon = make_placeholder_icon();

        let tray = TrayIconBuilder::new()
            .with_menu(Box::new(menu))
            .with_tooltip("Nimbus — loading…")
            .with_icon(icon)
            .build()?;

        Ok(Self {
            _icon: tray,
            id_settings,
            id_refresh,
            id_quit,
        })
    }

    /// Update the tooltip text (shown on hover)
    pub fn set_tooltip(&self, text: &str) {
        if let Err(e) = self._icon.set_tooltip(Some(text)) {
            log::warn!("Failed to set tray tooltip: {e}");
        }
    }

    /// Poll the global MenuEvent channel — call this every frame from the event loop
    pub fn poll_menu(&self) -> Option<MenuEvent> {
        MenuEvent::receiver().try_recv().ok()
    }
}

/// 16×16 gray RGBA square — replace with a real icon via include_bytes!
fn make_placeholder_icon() -> tray_icon::Icon {
    const SIZE: u32 = 16;
    let rgba = vec![180u8, 180, 180, 220].repeat((SIZE * SIZE) as usize);
    tray_icon::Icon::from_rgba(rgba, SIZE, SIZE).expect("Failed to create placeholder icon")
}
