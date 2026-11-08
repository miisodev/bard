use tauri::{
    menu::{CheckMenuItem, Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
};

/// The tray entry. Left click opens the main window; the menu holds
/// everything else.
///
/// On Linux the entry is a StatusNotifierItem of Bard's own (`linux_tray`).
/// Tauri's Linux tray goes through libappindicator, which opens the menu on
/// every click and never reports a left click, so the shelf could not be
/// opened from the icon there. A host without a StatusNotifier watcher falls
/// back to the Tauri tray, which at least offers Show Bard in its menu.
pub fn create(app: &tauri::App) -> tauri::Result<()> {
    #[cfg(target_os = "linux")]
    {
        let handle = app.handle().clone();
        let fallback = app.handle().clone();
        tauri::async_runtime::spawn(async move {
            if let Err(error) = linux_tray::spawn(handle).await {
                log::warn!("StatusNotifier tray unavailable, using the menu tray: {error}");
                let menu_app = fallback.clone();
                let _ = fallback.run_on_main_thread(move || {
                    if let Err(error) = create_menu_tray(&menu_app) {
                        log::warn!("tray unavailable: {error}");
                    }
                });
            }
        });
        Ok(())
    }
    #[cfg(not(target_os = "linux"))]
    {
        create_menu_tray(app.handle()).map(|_| ())
    }
}

/// What a tray menu entry does, shared by both tray implementations.
fn run_action(app: &tauri::AppHandle, id: &str) {
    match id {
        "show" => crate::panel::show_now(app),
        "settings" => crate::commands::open_settings(app.clone()),
        "draw" => crate::commands::open_draw(app.clone()),
        "guide" => crate::panel::show_guide(app),
        "capture" => {
            let app = app.clone();
            std::thread::spawn(move || {
                crate::monitor::toggle_paused(&app);
            });
        }
        "dictate" => {
            let app = app.clone();
            std::thread::spawn(move || {
                if crate::dictation::is_recording() {
                    crate::dictation::stop(&app);
                } else {
                    let _ = crate::dictation::start(&app);
                }
            });
        }
        "autostart" => {
            let next = !db_autostart_handle(app);
            let app = app.clone();
            tauri::async_runtime::spawn(async move {
                if let Err(error) = crate::commands::set_autostart(app.clone(), next).await {
                    use tauri::Emitter;
                    let _ = app.emit_to("main", "capture-failed", error);
                    crate::panel::show_now(&app);
                }
            });
        }
        "quit" => {
            crate::crash::record_event("app:quit");
            crate::guardian::mark_clean_exit();
            app.exit(0);
        }
        _ => {}
    }
}

fn tooltip() -> &'static str {
    if crate::monitor::is_paused() {
        "Bard — clipboard capture paused"
    } else {
        "Bard — clipboard, draw and dictate"
    }
}

/// The Tauri tray's capture checkmark, kept to follow the pause switch.
static CAPTURE_ITEM: std::sync::OnceLock<CheckMenuItem<tauri::Wry>> = std::sync::OnceLock::new();

/// Bring the tray in line with the pause switch after it changed anywhere.
pub fn sync(app: &tauri::AppHandle) {
    if let Some(item) = CAPTURE_ITEM.get() {
        let _ = item.set_checked(!crate::monitor::is_paused());
    }
    if let Some(tray) = app.tray_by_id("bard-tray") {
        let _ = tray.set_tooltip(Some(tooltip()));
    }
    #[cfg(target_os = "linux")]
    linux_tray::refresh();
}

fn create_menu_tray(app: &tauri::AppHandle) -> tauri::Result<tauri::tray::TrayIcon> {
    let show = MenuItem::with_id(app, "show", "Show Bard", true, None::<&str>)?;
    let draw = MenuItem::with_id(app, "draw", "Draw", true, None::<&str>)?;
    let settings = MenuItem::with_id(app, "settings", "Settings", true, None::<&str>)?;
    let guide = MenuItem::with_id(app, "guide", "Shortcuts", true, None::<&str>)?;
    let dictate = MenuItem::with_id(app, "dictate", "Dictate", true, None::<&str>)?;
    let capture = CheckMenuItem::with_id(
        app,
        "capture",
        "Keep what I copy",
        true,
        !crate::monitor::is_paused(),
        None::<&str>,
    )?;
    let autostart = CheckMenuItem::with_id(
        app,
        "autostart",
        "Start at login",
        true,
        db_autostart_handle(app),
        None::<&str>,
    )?;
    let quit = MenuItem::with_id(app, "quit", "Quit Bard", true, None::<&str>)?;
    let separator = || tauri::menu::PredefinedMenuItem::separator(app);
    let menu = Menu::with_items(
        app,
        &[
            &show,
            &draw,
            &settings,
            &guide,
            &separator()?,
            &dictate,
            &capture,
            &separator()?,
            &autostart,
            &quit,
        ],
    )?;
    let _ = CAPTURE_ITEM.set(capture.clone());

    let icon = app.default_window_icon().cloned().unwrap();

    let builder = TrayIconBuilder::with_id("bard-tray")
        .icon(icon)
        .tooltip(tooltip())
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                crate::panel::show_now(tray.app_handle());
            }
        })
        .on_menu_event(|app, event| run_action(app, event.id().as_ref()));

    builder.build(app)
}

fn db_autostart_handle(app: &tauri::AppHandle) -> bool {
    crate::db::get_setting(app, "autostart_enabled") == Some("1".into())
}

/// Bard's StatusNotifierItem on Linux: the desktop calls `activate` on a left
/// click, which opens the shelf, and shows the menu on a right click.
#[cfg(target_os = "linux")]
mod linux_tray {
    use ksni::{
        menu::{CheckmarkItem, StandardItem},
        Icon, MenuItem, ToolTip, Tray, TrayMethods,
    };
    use std::sync::OnceLock;

    static HANDLE: OnceLock<ksni::Handle<BardTray>> = OnceLock::new();

    struct BardTray {
        app: tauri::AppHandle,
        icon: Vec<Icon>,
    }

    impl BardTray {
        fn act(&self, id: &'static str) {
            let app = self.app.clone();
            let _ = self
                .app
                .run_on_main_thread(move || super::run_action(&app, id));
        }

        fn item(label: &str, id: &'static str) -> MenuItem<Self> {
            StandardItem {
                label: label.into(),
                activate: Box::new(move |tray: &mut Self| tray.act(id)),
                ..Default::default()
            }
            .into()
        }
    }

    impl Tray for BardTray {
        fn id(&self) -> String {
            "bard".into()
        }

        fn title(&self) -> String {
            "Bard".into()
        }

        fn icon_pixmap(&self) -> Vec<Icon> {
            self.icon.clone()
        }

        fn tool_tip(&self) -> ToolTip {
            ToolTip {
                title: super::tooltip().into(),
                ..Default::default()
            }
        }

        fn activate(&mut self, _x: i32, _y: i32) {
            crate::panel::show_now(&self.app);
        }

        fn menu(&self) -> Vec<MenuItem<Self>> {
            vec![
                Self::item("Show Bard", "show"),
                Self::item("Draw", "draw"),
                Self::item("Settings", "settings"),
                Self::item("Shortcuts", "guide"),
                MenuItem::Separator,
                Self::item("Dictate", "dictate"),
                CheckmarkItem {
                    label: "Keep what I copy".into(),
                    checked: !crate::monitor::is_paused(),
                    activate: Box::new(|tray: &mut Self| tray.act("capture")),
                    ..Default::default()
                }
                .into(),
                MenuItem::Separator,
                CheckmarkItem {
                    label: "Start at login".into(),
                    checked: super::db_autostart_handle(&self.app),
                    activate: Box::new(|tray: &mut Self| tray.act("autostart")),
                    ..Default::default()
                }
                .into(),
                MenuItem::Separator,
                Self::item("Quit Bard", "quit"),
            ]
        }
    }

    /// The window icon as the ARGB32, network-byte-order pixmap the
    /// StatusNotifier specification asks for.
    fn pixmap(app: &tauri::AppHandle) -> Vec<Icon> {
        let Some(image) = app.default_window_icon() else {
            return Vec::new();
        };
        let (pixels, _) = image.rgba().as_chunks::<4>();
        let data = pixels
            .iter()
            .flat_map(|&[r, g, b, a]| [a, r, g, b])
            .collect();
        vec![Icon {
            width: image.width() as i32,
            height: image.height() as i32,
            data,
        }]
    }

    /// Re-read the menu and tooltip after the pause switch changed.
    pub fn refresh() {
        if let Some(handle) = HANDLE.get() {
            let handle = handle.clone();
            tauri::async_runtime::spawn(async move {
                let _ = handle.update(|_| {}).await;
            });
        }
    }

    pub async fn spawn(app: tauri::AppHandle) -> Result<(), ksni::Error> {
        let sandboxed = crate::packaging::channel() == "flatpak";
        let tray = BardTray {
            icon: pixmap(&app),
            app,
        };
        let handle = tray.disable_dbus_name(sandboxed).spawn().await?;
        let _ = HANDLE.set(handle);
        Ok(())
    }
}
