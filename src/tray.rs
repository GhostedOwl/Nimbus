use anyhow::Result;
use tray_icon::{
    menu::{Menu, MenuEvent, MenuItem, MenuId, PredefinedMenuItem},
    TrayIcon, TrayIconBuilder,
};

pub struct Tray {
    _icon: TrayIcon,
    pub id_settings: MenuId,
    pub id_refresh: MenuId,
    pub id_quit: MenuId,
}

impl Tray {
    pub fn build() -> Result<Self> {
        let item_settings = MenuItem::new("Налаштування", true, None);
        let item_refresh  = MenuItem::new("Оновити",      true, None);
        let item_quit     = MenuItem::new("Вихід",        true, None);

        let id_settings = item_settings.id().clone();
        let id_refresh  = item_refresh.id().clone();
        let id_quit     = item_quit.id().clone();

        let menu = Menu::new();
        menu.append(&item_settings)?;
        menu.append(&item_refresh)?;
        menu.append(&PredefinedMenuItem::separator())?;
        menu.append(&item_quit)?;

        // Мінімальна прозора іконка — текст показується через set_title
        let icon = minimal_icon();

        let tray = TrayIconBuilder::new()
            .with_menu(Box::new(menu))
            .with_tooltip("Nimbus")
            .with_icon(icon)
            .build()?;

        Ok(Self { _icon: tray, id_settings, id_refresh, id_quit })
    }

    pub fn set_tooltip(&self, text: &str) {
        if let Err(e) = self._icon.set_tooltip(Some(text)) {
            log::warn!("set_tooltip: {e}");
        }
    }

    pub fn set_title(&self, text: &str) {
        self._icon.set_title(Some(text));
    }

}

/// 1×1 прозорий піксель — вся інформація йде через set_title
fn minimal_icon() -> tray_icon::Icon {
    tray_icon::Icon::from_rgba(vec![0, 0, 0, 0], 1, 1).expect("icon")
}
