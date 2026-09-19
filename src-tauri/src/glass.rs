use tauri::{AppHandle, Manager};

/*
  Glass is a theme, not a permanent backdrop.

  Acrylic used to be applied unconditionally at alpha 170–190, which is opaque
  enough to read as a flat slab while still paying for a compositor effect
  nobody could see — and the web layer then painted its own solid background on
  top of it anyway. Now the effect is applied only for the Glass theme, at an
  alpha low enough to actually show the desktop through, and cleared otherwise.

  The web layer cooperates: `data-glass="1"` switches its surfaces to
  semi-transparent (see tokens.css). Both halves are required — a translucent
  window with opaque surfaces shows nothing, and translucent surfaces over a
  cleared window show the window's own background colour.
*/
pub fn apply_all(app: &AppHandle, theme: &str) {
    let Some(win) = app.get_webview_window("main") else {
        return;
    };

    if theme != "glass" {
        let _ = window_vibrancy::clear_acrylic(&win);
        return;
    }

    // A near-neutral tint: the acrylic supplies the colour by sampling what is
    // behind the window, so a strongly tinted overlay only muddies it.
    let dark = matches!(win.theme().unwrap_or(tauri::Theme::Dark), tauri::Theme::Dark);
    let tint = if dark {
        (22, 22, 28, 110)
    } else {
        (250, 250, 253, 110)
    };

    let _ = window_vibrancy::apply_acrylic(&win, Some(tint));
}
