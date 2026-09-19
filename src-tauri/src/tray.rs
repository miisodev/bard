use tauri::{
    menu::{CheckMenuItem, Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
};
use tauri_plugin_autostart::ManagerExt;

pub fn create(app: &tauri::App) -> tauri::Result<tauri::tray::TrayIcon> {
    let show = MenuItem::with_id(app, "show", "Show Bard", true, None::<&str>)?;
    let capture = MenuItem::with_id(app, "capture", "Capture selection", true, None::<&str>)?;
    let dictate = MenuItem::with_id(app, "dictate", "Dictate", true, None::<&str>)?;
    let autostart = CheckMenuItem::with_id(
        app,
        "autostart",
        "Start at login",
        true,
        db_autostart(app),
        None::<&str>,
    )?;
    let quit = MenuItem::with_id(app, "quit", "Quit Bard", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show, &capture, &dictate, &autostart, &quit])?;

    let icon = app.default_window_icon().cloned().unwrap();

    let builder = TrayIconBuilder::with_id("bard-tray")
        .icon(icon)
        .tooltip("Bard — double-tap Right Shift to open")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                crate::panel::toggle_now(tray.app_handle());
            }
        })
        .on_menu_event(|app, event| match event.id().as_ref() {
            "show" => crate::panel::show_now(app),
            "capture" => crate::capture::capture_selection(app),
            "dictate" => crate::dictation::start(app),
            "autostart" => {
                let next = !db_autostart_handle(app);
                let _ = if next {
                    app.autolaunch().enable()
                } else {
                    app.autolaunch().disable()
                };
                crate::db::set_setting(app, "autostart_enabled", if next { "1" } else { "0" });
            }
            "quit" => app.exit(0),
            _ => {}
        });

    builder.build(app)
}

fn db_autostart(app: &tauri::App) -> bool {
    crate::db::get_setting(app.handle(), "autostart_enabled") == Some("1".into())
}

fn db_autostart_handle(app: &tauri::AppHandle) -> bool {
    crate::db::get_setting(app, "autostart_enabled") == Some("1".into())
}