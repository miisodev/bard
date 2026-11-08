use std::{
    hash::{Hash, Hasher},
    sync::Mutex,
    thread,
    time::Duration,
};

// The clipboard format registration is a Windows-only path; everything that
// uses it has a `cfg(not(windows))` sibling below.
#[cfg(windows)]
use std::sync::OnceLock;

use tauri::{AppHandle, Emitter, Manager};

use crate::{commands::ImagesDir, db};

static LAST_IMAGE_HASH: Mutex<Option<u64>> = Mutex::new(None);
static LAST_OWN_SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

#[cfg(windows)]
#[link(name = "user32")]
extern "system" {
    fn GetClipboardSequenceNumber() -> u32;
    fn IsClipboardFormatAvailable(format: u32) -> i32;
    fn RegisterClipboardFormatW(name: *const u16) -> u16;
    fn GetClipboardData(format: u32) -> *mut std::ffi::c_void;
    fn RegisterClassW(class: *const WndClassW) -> u16;
    fn CreateWindowExW(
        ex_style: u32,
        class_name: *const u16,
        window_name: *const u16,
        style: u32,
        x: i32,
        y: i32,
        width: i32,
        height: i32,
        parent: *mut std::ffi::c_void,
        menu: *mut std::ffi::c_void,
        instance: *mut std::ffi::c_void,
        param: *mut std::ffi::c_void,
    ) -> *mut std::ffi::c_void;
    fn DefWindowProcW(
        window: *mut std::ffi::c_void,
        message: u32,
        wparam: usize,
        lparam: isize,
    ) -> isize;
    fn AddClipboardFormatListener(window: *mut std::ffi::c_void) -> i32;
    fn RemoveClipboardFormatListener(window: *mut std::ffi::c_void) -> i32;
    fn MsgWaitForMultipleObjects(
        count: u32,
        handles: *const *mut std::ffi::c_void,
        wait_all: i32,
        milliseconds: u32,
        wake_mask: u32,
    ) -> u32;
    fn PeekMessageW(
        message: *mut Msg,
        window: *mut std::ffi::c_void,
        filter_min: u32,
        filter_max: u32,
        remove: u32,
    ) -> i32;
    fn TranslateMessage(message: *const Msg) -> i32;
    fn DispatchMessageW(message: *const Msg) -> isize;
    fn CreateEventW(
        attributes: *mut std::ffi::c_void,
        manual_reset: i32,
        initial_state: i32,
        name: *const u16,
    ) -> *mut std::ffi::c_void;
    fn SetEvent(event: *mut std::ffi::c_void) -> i32;
    fn ResetEvent(event: *mut std::ffi::c_void) -> i32;
}

#[cfg(windows)]
#[link(name = "kernel32")]
extern "system" {
    fn GlobalSize(memory: *mut std::ffi::c_void) -> usize;
    fn GlobalLock(memory: *mut std::ffi::c_void) -> *mut std::ffi::c_void;
    fn GlobalUnlock(memory: *mut std::ffi::c_void) -> i32;
    fn GetModuleHandleW(name: *const u16) -> isize;
    fn CloseHandle(handle: isize) -> i32;
}

/// A message-only window that exists to be told when the clipboard changes.
#[cfg(windows)]
const HWND_MESSAGE: *mut std::ffi::c_void = -3isize as *mut std::ffi::c_void;

#[cfg(windows)]
const WM_CLIPBOARDUPDATE: u32 = 0x031D;

/// Wake on any input the thread's queue holds — the clipboard's notification
/// among them — and on the timeout.
#[cfg(windows)]
const QS_ALLINPUT: u32 = 0x04FF;

#[cfg(windows)]
const PM_REMOVE: u32 = 0x0001;

/// The window class the clipboard listener's hidden window is made from. Only
/// the members the window procedure needs are spelled out, in the order the C
/// struct declares them, because this is read by the operating system.
#[cfg(windows)]
#[repr(C)]
struct WndClassW {
    style: u32,
    lpfn_wnd_proc:
        Option<unsafe extern "system" fn(*mut std::ffi::c_void, u32, usize, isize) -> isize>,
    cb_cls_extra: i32,
    cb_wnd_extra: i32,
    h_instance: *mut std::ffi::c_void,
    h_icon: *mut std::ffi::c_void,
    h_cursor: *mut std::ffi::c_void,
    hbr_background: *mut std::ffi::c_void,
    lpsz_menu_name: *const u16,
    lpsz_class_name: *const u16,
}

/// The public shape of `MSG` on 64-bit Windows: handle, message, the two
/// parameters, the tick and the point.
#[cfg(windows)]
#[repr(C)]
struct Msg {
    hwnd: *mut std::ffi::c_void,
    message: u32,
    w_param: usize,
    l_param: isize,
    time: u32,
    point: [i32; 2],
}

/// Record an image Bard put on the clipboard itself.
pub fn note_own_image(bytes: &[u8]) {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    bytes.hash(&mut hasher);
    *LAST_IMAGE_HASH
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(hasher.finish());
}

/// The person's pause switch. While it is on, nothing copied is stored.
static PAUSED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

pub const PAUSED_SETTING: &str = "capture_paused";

pub fn is_paused() -> bool {
    PAUSED.load(std::sync::atomic::Ordering::SeqCst)
}

/// Turn capture on or off, persist it, and say so. Returns the new state.
pub fn set_paused(app: &AppHandle, paused: bool) -> bool {
    PAUSED.store(paused, std::sync::atomic::Ordering::SeqCst);
    db::set_setting(app, PAUSED_SETTING, if paused { "1" } else { "0" });
    if !paused {
        // Whatever was copied while paused is not captured on resume: the
        // switch means "do not keep this", including the last thing copied.
        forget_current_clipboard();
    }
    crate::feedback::notify(
        app,
        if paused {
            crate::feedback::Note::info("pause", "Clipboard capture paused")
        } else {
            crate::feedback::Note::info("resume", "Clipboard capture on")
        },
    );
    let _ = app.emit("capture-paused", paused);
    crate::tray::sync(app);
    crate::commands::refresh(app);
    paused
}

pub fn toggle_paused(app: &AppHandle) -> bool {
    set_paused(app, !is_paused())
}

/// Mark the clipboard's current text and image as seen, so they are not
/// captured later as if they had just been copied.
fn forget_current_clipboard() {
    if let Ok(mut clipboard) = crate::clip::handle() {
        if let Ok(text) = clipboard.get_text() {
            note_own_text(text.trim());
        }
    }
}

pub fn note_own_write(app: &AppHandle) {
    #[cfg(windows)]
    {
        let seq = unsafe { GetClipboardSequenceNumber() };
        LAST_OWN_SEQ.store(seq as u64, std::sync::atomic::Ordering::SeqCst);
    }
    let _ = app;
}

/// The largest clipboard text Bard will store, in bytes.
///
/// Nothing bounded this before. A single 200 MB copy out of a log viewer was
/// written to the database and then re-serialised into every open webview on
/// every refresh event, which left the panel permanently unresponsive with no
/// way back short of deleting the item by hand. Bard is a background agent that
/// must never be the reason a machine feels wrong, so an oversized payload is
/// refused with a visible reason rather than absorbed.
pub const MAX_CAPTURE_TEXT_BYTES: usize = 256 * 1024;

/// Hash of the last captured text, rather than the text itself.
///
/// The previous code kept a full `String` copy of the most recent payload alive
/// for the lifetime of the process, purely to compare against the next one. A
/// 64-bit hash answers the same question — has this exact text been captured
/// already — without retaining a user's clipboard contents in memory
/// indefinitely.
static LAST_CONTENT_HASH: Mutex<Option<u64>> = Mutex::new(None);

fn content_hash(text: &str) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    text.hash(&mut hasher);
    hasher.finish()
}

/// Record content Bard put on the clipboard itself, so the watcher does not
/// capture it back as if the person had copied it.
///
/// Dictation writes the transcript to the clipboard and then files it in the
/// timeline, going through `insert_item_with_origin` rather than through
/// `capture_text`. Without this the watcher had never seen that text, so every
/// dictated line came back a moment later as a second `clipboard` row.
pub fn note_own_text(text: &str) {
    let digest = content_hash(text);
    let mut guard = LAST_CONTENT_HASH
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    *guard = Some(digest);
}

pub fn capture_text(app: &AppHandle, text: &str) {
    if crate::license::require(app).is_err() {
        return;
    }
    let digest = content_hash(text);
    {
        // A poisoned lock must not kill capture for the rest of the session, so
        // degrade to "have not seen this" rather than panicking the monitor.
        let mut guard = LAST_CONTENT_HASH
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if *guard == Some(digest) {
            return;
        }
        *guard = Some(digest);
    }

    if text.len() > MAX_CAPTURE_TEXT_BYTES {
        crate::crash::record_event("capture:text-too-large");
        crate::feedback::notify(
            app,
            crate::feedback::Note::error(format!(
                "Too large to keep ({} KB; the limit is {} KB)",
                text.len() / 1024,
                MAX_CAPTURE_TEXT_BYTES / 1024
            )),
        );
        return;
    }

    // Copying something Bard already keeps moves it to the top rather than
    // filing a second copy; a pinned or kept copy simply becomes current.
    if let Some(kept) = db::take_duplicate(app, text, "text") {
        db::set_active_item(app, Some(kept));
        crate::commands::refresh(app);
        crate::feedback::notify(app, crate::feedback::Note::copied(text));
        return;
    }
    let id = db::insert_item_with_origin(app, text, "text", None, "clipboard");
    if id == db::INSERT_FAILED {
        // The write did not happen. `insert_item_with_origin` has already
        // reported it; emitting `captured` here would tell four webviews an item
        // exists that is not in the timeline.
        return;
    }
    // The happy path used to leave no breadcrumb at all, so the log could not tell
    // a successful capture from a gesture that did nothing — the same gap
    // `capture:no-selection` was written to close, still open here.
    crate::crash::record_event("capture:stored");
    db::set_active_item(app, Some(id));
    crate::feedback::notify(app, crate::feedback::Note::copied(text));
    // Age-based retention is the only automatic deletion. The old call to
    // `prune_history` here deleted the oldest automatic items past 5,000 on
    // every single capture, including when the user had chosen Forever.
    let mut removed = 0;
    match db::cleanup_origin(app, "clipboard") {
        Ok(count) => removed = count,
        Err(error) => {
            crate::crash::record_event("capture:retention-failed");
            let _ = app.emit_to("main", "capture-failed", error);
        }
    }
    announce_captured(app, id, removed);
}

/// Tell the panel about a capture, and only about the capture.
///
/// This used to be `emit("captured", id)` plus `emit("refresh")` broadcast to
/// every window, and every window answered the refresh by re-reading the whole
/// timeline — so the price of one copy grew with the size of the library, and
/// four webviews paid it. The item itself now travels in the event: it is one
/// row, it has just been written, and it is exactly what the panel would have
/// read. `removed` is what the same capture's retention pass took away, which
/// is the one case the splice cannot express; there the panel reads instead.
fn announce_captured(app: &AppHandle, id: i64, removed: usize) {
    let item = db::item_by_id(app, id);
    if item.is_none() {
        // The row is gone again already (a concurrent delete). Nothing to
        // splice, so the panel has to read.
        crate::commands::refresh(app);
    }
    if removed > 0 {
        crate::commands::refresh(app);
    }
    let _ = app.emit_to("main", "captured", Captured { id, item, removed });
}

/// The payload the panel splices into its state. `id` alone is what the sound
/// effect needs; `item` and `removed` are what make a read unnecessary.
#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct Captured {
    id: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    item: Option<db::Item>,
    removed: usize,
}

/// Poll the clipboard for new content.
///
/// This loop is the product. It had no heartbeat and no supervision, so when it
/// stopped — a panicking iteration, a poisoned lock — capture was simply gone
/// with nothing anywhere saying so, which is the same class of silent failure the
/// raw-input listener had. Each iteration is now panic-contained so one bad poll
/// cannot end capture for the session, and a periodic heartbeat records that the
/// loop is alive.
///
/// On Windows the loop is woken by the clipboard itself: a hidden window holds
/// an `AddClipboardFormatListener` registration, `WM_CLIPBOARDUPDATE` sets an
/// event, and the wait returns on the spot. The poll stays as the net under it —
/// [`SAFETY_POLL_MS`], long enough that it costs nothing and short enough that
/// a listener that never fires is a slowdown rather than a dead product. When
/// the listener cannot be installed at all (Linux, or a failed
/// `CreateWindowExW`) the loop falls back to [`FALLBACK_POLL_MS`], which is the
/// interval a build without any of this machinery would have.
pub fn start(app: AppHandle) {
    PAUSED.store(
        db::get_setting(&app, PAUSED_SETTING).as_deref() == Some("1"),
        std::sync::atomic::Ordering::SeqCst,
    );
    // Whatever is on the clipboard at launch was copied before Bard was
    // watching; it is not a new copy.
    forget_current_clipboard();
    thread::spawn(move || {
        let mut last_seq = clip_seq();
        let mut ticks: u64 = 0;
        let listener = ClipboardListener::install();
        if listener.is_none() {
            crate::crash::record_event("monitor:no-change-listener");
        }
        let poll_ms = if listener.is_some() {
            SAFETY_POLL_MS
        } else {
            FALLBACK_POLL_MS
        };
        loop {
            match &listener {
                Some(listener) => listener.wait(poll_ms),
                None => thread::sleep(Duration::from_millis(poll_ms)),
            }
            ticks += 1;
            if ticks.is_multiple_of(HEARTBEAT_TICKS) {
                crate::crash::record_event("monitor:alive");
            }
            let polled = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                // With a change listener the sequence says whether anything
                // happened. Without one (no X server on Linux) every tick reads
                // the text and lets the content hash decide.
                let seq = clip_seq();
                if listener.is_some() || cfg!(windows) {
                    if seq == last_seq {
                        return;
                    }
                    last_seq = seq;
                    if seq as u64 == LAST_OWN_SEQ.load(std::sync::atomic::Ordering::SeqCst) {
                        return;
                    }
                }
                if is_paused() || clip_excluded() {
                    return;
                }
                let watched = listener.is_some() || cfg!(windows);
                let (files, text, image) = {
                    let Ok(mut cb) = crate::clip::handle() else {
                        return;
                    };
                    let files = if watched {
                        cb.get().file_list().ok().filter(|f| !f.is_empty())
                    } else {
                        None
                    };
                    let text = if files.is_none() {
                        cb.get_text().ok()
                    } else {
                        None
                    };
                    let image = if files.is_none() && text.is_none() && watched {
                        cb.get_image().ok()
                    } else {
                        None
                    };
                    (files, text, image)
                };
                if let Some(files) = files {
                    capture_files(&app, files);
                } else if let Some(text) = text {
                    let t = text.trim().to_string();
                    if t.is_empty() {
                        return;
                    }
                    // A change that landed while the text was being read is
                    // the next tick's to capture, not this one's.
                    if cfg!(windows) && clip_seq() != seq {
                        return;
                    }
                    capture_text(&app, &t);
                } else if let Some(img) = image {
                    let _ = capture_image(&app, img);
                }
            }));
            if polled.is_err() {
                crate::crash::record_event("monitor:iteration-failed");
            }
        }
    });
}

const SAFETY_POLL_MS: u64 = 250;

/// The interval used when there is no listener to wait on. 75 ms, not 150: the
/// check the sleep gates is `clip_seq`, a single cheap API call with no
/// allocation that runs before any clipboard read, so the poll is a heartbeat
/// rather than work; and halving it halves the one delay the user can feel,
/// between the copy and the item appearing.
const FALLBACK_POLL_MS: u64 = 500;

/// Roughly every two and a half minutes at the safety-poll interval, and sooner
/// under real use. An idle install writes one line; this is a liveness marker,
/// not a log of activity.
const HEARTBEAT_TICKS: u64 = 600;

/// The clipboard's own notification, on Windows: a hidden window registered
/// with `AddClipboardFormatListener`, an event it sets, and a wait the copy
/// wakes immediately.
///
/// `None` from [`ClipboardListener::install`] is not a failure to report — the
/// loop's poll covers it — and every step here is best-effort for that reason.
#[cfg(windows)]
struct ClipboardListener {
    event: *mut std::ffi::c_void,
    window: *mut std::ffi::c_void,
}

#[cfg(windows)]
impl Drop for ClipboardListener {
    /// The listener lives for the process's life, so this is the honest shape of
    /// the ownership rather than a path anything takes: it exists so the window
    /// and the event are released by whoever owns them if that ever changes.
    fn drop(&mut self) {
        unsafe {
            CLIPBOARD_EVENT.store(0, std::sync::atomic::Ordering::SeqCst);
            let _ = RemoveClipboardFormatListener(self.window);
            let _ = CloseHandle(self.event as isize);
        }
    }
}

#[cfg(windows)]
impl ClipboardListener {
    fn install() -> Option<ClipboardListener> {
        unsafe {
            let class_name: Vec<u16> = "BardClipboardListener\0".encode_utf16().collect();
            let instance = GetModuleHandleW(std::ptr::null());
            let class = WndClassW {
                style: 0,
                lpfn_wnd_proc: Some(clipboard_wnd_proc),
                cb_cls_extra: 0,
                cb_wnd_extra: 0,
                h_instance: instance as *mut std::ffi::c_void,
                h_icon: std::ptr::null_mut(),
                h_cursor: std::ptr::null_mut(),
                hbr_background: std::ptr::null_mut(),
                lpsz_menu_name: std::ptr::null(),
                lpsz_class_name: class_name.as_ptr(),
            };
            // A zero return means the class already exists, which is fine: the
            // window below is what matters.
            RegisterClassW(&class);
            let event = CreateEventW(std::ptr::null_mut(), 1, 0, std::ptr::null());
            if event.is_null() {
                return None;
            }
            CLIPBOARD_EVENT.store(event as isize, std::sync::atomic::Ordering::SeqCst);
            let window = CreateWindowExW(
                0,
                class_name.as_ptr(),
                std::ptr::null(),
                0,
                0,
                0,
                0,
                0,
                HWND_MESSAGE,
                std::ptr::null_mut(),
                instance as *mut std::ffi::c_void,
                std::ptr::null_mut(),
            );
            if window.is_null() || AddClipboardFormatListener(window) == 0 {
                CLIPBOARD_EVENT.store(0, std::sync::atomic::Ordering::SeqCst);
                let _ = CloseHandle(event as isize);
                return None;
            }
            Some(ClipboardListener { event, window })
        }
    }

    /// Block until the clipboard changes or `timeout_ms` passes, then let the
    /// caller decide what to do — which is the same check either way, so the
    /// event's own state is not part of the answer.
    fn wait(&self, timeout_ms: u64) {
        unsafe {
            let handles = [self.event];
            let _ =
                MsgWaitForMultipleObjects(1, handles.as_ptr(), 0, timeout_ms as u32, QS_ALLINPUT);
            let _ = ResetEvent(self.event);
            // Anything the window's queue holds is drained so the next wait can
            // block; nothing here needs dispatching, since the one message that
            // matters is sent to the window procedure directly.
            let mut message = std::mem::zeroed::<Msg>();
            while PeekMessageW(&mut message, std::ptr::null_mut(), 0, 0, PM_REMOVE) != 0 {
                let _ = TranslateMessage(&message);
                DispatchMessageW(&message);
            }
        }
    }
}

#[cfg(not(windows))]
struct ClipboardListener;

#[cfg(not(windows))]
impl ClipboardListener {
    fn install() -> Option<ClipboardListener> {
        #[cfg(target_os = "linux")]
        if x11watch::spawn() {
            return Some(ClipboardListener);
        }
        None
    }

    fn wait(&self, timeout_ms: u64) {
        #[cfg(target_os = "linux")]
        x11watch::wait(Duration::from_millis(timeout_ms));
        #[cfg(not(target_os = "linux"))]
        thread::sleep(Duration::from_millis(timeout_ms));
    }
}

/// Clipboard ownership changes on X11 and XWayland, through XFixes.
///
/// GNOME on Wayland mirrors every Wayland copy into XWayland's CLIPBOARD
/// selection, so one X11 watcher hears copies from native Wayland apps and X11
/// apps alike. Each change also reads the offered TARGETS once: a password
/// manager marks a secret with `x-kde-passwordManagerHint`, and that copy is
/// never stored.
#[cfg(target_os = "linux")]
mod x11watch {
    use std::sync::{Condvar, Mutex};
    use std::time::{Duration, Instant};

    use x11rb::connection::Connection;
    use x11rb::protocol::xfixes::{ConnectionExt as _, SelectionEventMask};
    use x11rb::protocol::xproto::{
        Atom, AtomEnum, ConnectionExt as _, CreateWindowAux, EventMask, Window, WindowClass,
    };
    use x11rb::protocol::Event;

    struct Changes {
        seq: u32,
        excluded: bool,
    }

    static CHANGES: Mutex<Changes> = Mutex::new(Changes {
        seq: 0,
        excluded: false,
    });
    static CHANGED: Condvar = Condvar::new();

    pub fn seq() -> u32 {
        CHANGES.lock().map(|c| c.seq).unwrap_or(0)
    }

    pub fn excluded() -> bool {
        CHANGES.lock().map(|c| c.excluded).unwrap_or(false)
    }

    pub fn wait(timeout: Duration) {
        let Ok(guard) = CHANGES.lock() else {
            std::thread::sleep(timeout);
            return;
        };
        let seen = guard.seq;
        let _ = CHANGED.wait_timeout_while(guard, timeout, |c| c.seq == seen);
    }

    struct Atoms {
        clipboard: Atom,
        targets: Atom,
        property: Atom,
        secret_hint: Atom,
    }

    fn intern(conn: &impl Connection, name: &[u8]) -> Option<Atom> {
        Some(conn.intern_atom(false, name).ok()?.reply().ok()?.atom)
    }

    /// True when the watcher is running. Fails closed to polling when there
    /// is no X server, no XFixes, or the window cannot be made.
    pub fn spawn() -> bool {
        let Ok((conn, screen)) = x11rb::connect(None) else {
            return false;
        };
        let has_xfixes = conn
            .xfixes_query_version(5, 0)
            .ok()
            .and_then(|cookie| cookie.reply().ok())
            .is_some();
        if !has_xfixes {
            return false;
        }
        let root = conn.setup().roots[screen].root;
        let Ok(window) = conn.generate_id() else {
            return false;
        };
        let made = conn
            .create_window(
                0,
                window,
                root,
                -10,
                -10,
                1,
                1,
                0,
                WindowClass::INPUT_ONLY,
                0,
                &CreateWindowAux::new()
                    .override_redirect(1)
                    .event_mask(EventMask::PROPERTY_CHANGE),
            )
            .ok()
            .and_then(|cookie| cookie.check().ok())
            .is_some();
        if !made {
            return false;
        }
        let atoms = match (
            intern(&conn, b"CLIPBOARD"),
            intern(&conn, b"TARGETS"),
            intern(&conn, b"BARD_TARGETS"),
            intern(&conn, b"x-kde-passwordManagerHint"),
        ) {
            (Some(clipboard), Some(targets), Some(property), Some(secret_hint)) => Atoms {
                clipboard,
                targets,
                property,
                secret_hint,
            },
            _ => return false,
        };
        let selected = conn
            .xfixes_select_selection_input(
                window,
                atoms.clipboard,
                SelectionEventMask::SET_SELECTION_OWNER,
            )
            .ok()
            .and_then(|cookie| cookie.check().ok())
            .is_some();
        if !selected || conn.flush().is_err() {
            return false;
        }
        std::thread::spawn(move || run(conn, window, atoms));
        true
    }

    fn run(conn: impl Connection, window: Window, atoms: Atoms) {
        loop {
            let event = match conn.wait_for_event() {
                Ok(event) => event,
                Err(_) => {
                    crate::crash::record_event("monitor:x11-connection-lost");
                    return;
                }
            };
            if let Event::XfixesSelectionNotify(notify) = event {
                let excluded = offers_secret(&conn, window, &atoms, notify.timestamp);
                if let Ok(mut changes) = CHANGES.lock() {
                    changes.seq = changes.seq.wrapping_add(1);
                    changes.excluded = excluded;
                }
                CHANGED.notify_all();
            }
        }
    }

    /// Ask the new owner what it offers and look for the password hint. A slow
    /// or silent owner is given 250 ms and then treated as an ordinary copy.
    fn offers_secret(conn: &impl Connection, window: Window, atoms: &Atoms, time: u32) -> bool {
        let asked = conn
            .convert_selection(window, atoms.clipboard, atoms.targets, atoms.property, time)
            .is_ok()
            && conn.flush().is_ok();
        if !asked {
            return false;
        }
        let deadline = Instant::now() + Duration::from_millis(250);
        loop {
            match conn.poll_for_event() {
                Ok(Some(Event::SelectionNotify(reply))) if reply.selection == atoms.clipboard => {
                    if reply.property == x11rb::NONE {
                        return false;
                    }
                    let Some(property) = conn
                        .get_property(true, window, atoms.property, AtomEnum::ATOM, 0, 4096)
                        .ok()
                        .and_then(|cookie| cookie.reply().ok())
                    else {
                        return false;
                    };
                    return property
                        .value32()
                        .map(|mut targets| targets.any(|t| t == atoms.secret_hint))
                        .unwrap_or(false);
                }
                Ok(Some(Event::XfixesSelectionNotify(_))) => {
                    // A newer copy arrived while this one was being asked
                    // about. Count it; the reader takes whatever is current.
                    if let Ok(mut changes) = CHANGES.lock() {
                        changes.seq = changes.seq.wrapping_add(1);
                    }
                }
                Ok(Some(_)) => {}
                Ok(None) => {
                    if Instant::now() >= deadline {
                        return false;
                    }
                    std::thread::sleep(Duration::from_millis(4));
                }
                Err(_) => return false,
            }
        }
    }

    #[cfg(test)]
    mod x11_integration {
        /// Run under a real X server (CI: xvfb-run): a change of clipboard
        /// owner is heard, which is what Linux capture depends on.
        #[test]
        #[ignore = "needs an X server"]
        fn x11_integration_clipboard_changes_are_heard() {
            assert!(super::spawn(), "XFixes watcher starts");
            let before = super::seq();
            let mut clipboard = arboard::Clipboard::new().expect("clipboard");
            clipboard
                .set_text("bard x11 integration".to_string())
                .expect("write");
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
            while super::seq() == before && std::time::Instant::now() < deadline {
                super::wait(std::time::Duration::from_millis(100));
            }
            assert_ne!(super::seq(), before, "the ownership change was heard");
            assert!(
                !super::excluded(),
                "an ordinary copy is not treated as secret"
            );
        }
    }
}

#[cfg(windows)]
static CLIPBOARD_EVENT: std::sync::atomic::AtomicIsize = std::sync::atomic::AtomicIsize::new(0);

#[cfg(windows)]
unsafe extern "system" fn clipboard_wnd_proc(
    window: *mut std::ffi::c_void,
    message: u32,
    wparam: usize,
    lparam: isize,
) -> isize {
    if message == WM_CLIPBOARDUPDATE {
        let event = CLIPBOARD_EVENT.load(std::sync::atomic::Ordering::SeqCst);
        if event != 0 {
            let _ = SetEvent(event as *mut std::ffi::c_void);
        }
        return 0;
    }
    DefWindowProcW(window, message, wparam, lparam)
}

/// Accept a clipboard image, or recognise one that is already in hand.
///
/// The cheap decisions stay here, on the thread that has to notice the next
/// copy: the licence, the hash that suppresses a re-capture of the same
/// picture. Everything that scales with the picture's size — the PNG encode,
/// the file write, the row and the events — goes to [`image_jobs`], because a
/// 4K screenshot is 100–250 ms of that thread's attention and the clipboard
/// does not wait. Taking `img` by value is what makes the hand-off free: the
/// DIB is already an owned buffer, so the worker holds it rather than a copy.
///
/// `true` now means "accepted for storage" rather than "written". The one
/// caller that reads the value (the double-Shift gesture) uses it to decide
/// whether to put the clipboard back, which is the same decision either way.
pub fn capture_image(app: &AppHandle, img: arboard::ImageData<'static>) -> bool {
    if crate::license::require(app).is_err() {
        return false;
    }
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    img.bytes.hash(&mut hasher);
    let hash = hasher.finish();
    {
        let mut guard = LAST_IMAGE_HASH
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if *guard == Some(hash) {
            return true;
        }
        *guard = Some(hash);
    }
    let job = ImageJob {
        app: app.clone(),
        hash,
        img,
    };
    match image_jobs().try_send(job) {
        Ok(()) => true,
        Err(std::sync::mpsc::TrySendError::Full(_)) => {
            // Full: two images are already being encoded and a third arrived.
            // Refusing is deliberate — the alternative is an unbounded queue of
            // multi-megabyte buffers — and the hash is forgotten so copying the
            // same picture again is not suppressed by a job that never ran.
            crate::crash::record_event("capture:image-busy");
            forget_image_hash(hash);
            false
        }
        Err(std::sync::mpsc::TrySendError::Disconnected(_)) => {
            // The worker is gone, which no path in this process produces. Same
            // answer as a full queue: the hash goes back so the next copy of
            // this picture is tried again.
            crate::crash::record_event("capture:image-worker-gone");
            forget_image_hash(hash);
            false
        }
    }
}

/// One image on its way to disk.
struct ImageJob {
    app: AppHandle,
    hash: u64,
    img: arboard::ImageData<'static>,
}

/// The encoder worker: one thread, two slots.
///
/// One thread keeps the jobs in the order the clipboard produced them, which is
/// what the hash comparison above assumes. Two slots bound the memory a burst
/// can hold (each can be a 4K DIB), and they are enough for every real use: an
/// image the person copied is written in a fraction of a second.
fn image_jobs() -> &'static std::sync::mpsc::SyncSender<ImageJob> {
    static JOBS: std::sync::OnceLock<std::sync::mpsc::SyncSender<ImageJob>> =
        std::sync::OnceLock::new();
    JOBS.get_or_init(|| {
        let (sender, receiver) = std::sync::mpsc::sync_channel::<ImageJob>(2);
        thread::spawn(move || {
            while let Ok(job) = receiver.recv() {
                // Contained, exactly as the work was while it ran inside the
                // monitor's own `catch_unwind`: under `panic = "abort"` a panic
                // here would end the process, and a picture Bard cannot encode
                // is not a reason to take capture down with it.
                let stored = std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || {
                    store_image(job)
                }));
                if stored.is_err() {
                    crate::crash::record_event("capture:image-store-failed");
                }
            }
        });
        sender
    })
}

/// Forget the last captured image, so the next copy of it is not suppressed.
///
/// Only when the stored hash is still `hash`: a failure in the worker must not
/// erase a *newer* capture's hash and let that picture be stored twice.
fn forget_image_hash(hash: u64) {
    let mut guard = LAST_IMAGE_HASH
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if *guard == Some(hash) {
        *guard = None;
    }
}

/// Encode, write, file and announce one clipboard image — off the monitor
/// thread. The failure paths here are the same ones the old inline body had:
/// a refused or failed write forgets the hash so the picture can be captured
/// again, and a PNG with no row is removed rather than left for a sweep that
/// would never find it.
fn store_image(job: ImageJob) {
    let ImageJob { app, hash, img } = job;
    let Some(encoded) = encode_png(&img) else {
        forget_image_hash(hash);
        return;
    };
    let images = app.state::<ImagesDir>().0.clone();
    let path = images.join(format!(
        "{}-{}.png",
        db::now_ms(),
        std::process::id() & 0xFFFF
    ));
    if std::fs::write(&path, encoded).is_err() {
        forget_image_hash(hash);
        return;
    }
    let id = db::insert_item_with_origin(
        &app,
        "",
        "image",
        Some(&path.to_string_lossy()),
        "clipboard",
    );
    if id == db::INSERT_FAILED {
        // The PNG is on disk with no row referencing it. Remove it rather than
        // leaving an orphan no sweep will ever collect.
        let _ = std::fs::remove_file(&path);
        return;
    }
    db::set_active_item(&app, Some(id));
    let mut removed = 0;
    match db::cleanup_origin(&app, "clipboard") {
        Ok(count) => removed = count,
        Err(error) => {
            crate::crash::record_event("capture:retention-failed");
            let _ = app.emit_to("main", "capture-failed", error);
        }
    }
    // Stored: the dedupe keeps the job's hash, so the same picture copied
    // again is recognised rather than written a second time.
    announce_captured(&app, id, removed);
    crate::feedback::notify(&app, crate::feedback::Note::info("copy", "Image copied"));
}

/// Files copied in a file manager are kept as file items.
fn capture_files(app: &AppHandle, files: Vec<std::path::PathBuf>) {
    if crate::license::require(app).is_err() {
        return;
    }
    let paths: Vec<String> = files
        .iter()
        .map(|f| f.to_string_lossy().into_owned())
        .collect();
    let digest = content_hash(&paths.join("\n"));
    {
        let mut guard = LAST_CONTENT_HASH
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if *guard == Some(digest) {
            return;
        }
        *guard = Some(digest);
    }
    // A reference, not a copy: Bard keeps where the files are, so copying the
    // item later hands the same files back to a file manager. It never owns
    // or deletes them.
    let joined = paths.join("\n");
    let id = match db::take_duplicate(app, &joined, "file") {
        Some(kept) => kept,
        None => db::insert_item_with_origin(app, &joined, "file", None, "clipboard"),
    };
    if id == db::INSERT_FAILED {
        return;
    }
    db::set_active_item(app, Some(id));
    let _ = db::cleanup_origin(app, "clipboard");
    crate::commands::refresh(app);
    crate::crash::record_event("capture:files-stored");
    let label = if paths.len() == 1 {
        std::path::Path::new(&paths[0])
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "File copied".into())
    } else {
        format!("{} files copied", paths.len())
    };
    crate::feedback::notify(
        app,
        crate::feedback::Note::info("copy", "Copied").with_detail(label),
    );
}

/// The largest image Bard will store, by pixel count and by encoded size.
///
/// A 4K screenshot is tens of megabytes of raw DIB, and the encode allocated an
/// unbounded `Vec` on the monitor thread — so one large screenshot meant tens of
/// megabytes and tens of milliseconds of work on the thread that also has to
/// notice the next copy. Both limits are checked before any of that happens.
pub const MAX_CAPTURE_IMAGE_PIXELS: u64 = 40_000_000;
pub const MAX_CAPTURE_IMAGE_BYTES: usize = 25 * 1024 * 1024;

/// A `Vec` that refuses to grow past a limit.
///
/// The byte cap has to be enforced *while* the encoder runs, not after it
/// returns. A 40 MP screenshot is comfortably inside the pixel limit and its PNG
/// can still exceed 25 MB, so a post-encode check rejects the image only after the
/// allocation and the CPU have already been spent — which is the entire cost the
/// limit exists to avoid, paid on the monitor thread that also has to notice the
/// next copy.
struct CappedBytes {
    bytes: Vec<u8>,
    cap: usize,
}

impl CappedBytes {
    fn new(cap: usize) -> Self {
        Self {
            bytes: Vec::with_capacity(cap.min(1 << 20)),
            cap,
        }
    }
}

impl std::io::Write for CappedBytes {
    fn write(&mut self, buffer: &[u8]) -> std::io::Result<usize> {
        if self.bytes.len() + buffer.len() > self.cap {
            return Err(std::io::Error::other(
                "encoded image exceeds the size Bard stores",
            ));
        }
        self.bytes.extend_from_slice(buffer);
        Ok(buffer.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

pub(crate) fn encode_png(img: &arboard::ImageData) -> Option<Vec<u8>> {
    use image::{ExtendedColorType, ImageEncoder};
    let pixels = (img.width as u64).saturating_mul(img.height as u64);
    if pixels > MAX_CAPTURE_IMAGE_PIXELS {
        crate::crash::record_event("capture:image-too-large");
        return None;
    }
    let mut png = CappedBytes::new(MAX_CAPTURE_IMAGE_BYTES);
    let encoder = image::codecs::png::PngEncoder::new(&mut png);
    encoder
        .write_image(
            img.bytes.as_ref(),
            img.width as u32,
            img.height as u32,
            ExtendedColorType::Rgba8,
        )
        .ok()?;
    Some(png.bytes)
}

#[cfg(windows)]
fn clip_seq() -> u32 {
    unsafe { GetClipboardSequenceNumber() }
}

#[cfg(target_os = "linux")]
fn clip_seq() -> u32 {
    x11watch::seq()
}

#[cfg(not(any(windows, target_os = "linux")))]
fn clip_seq() -> u32 {
    0
}

#[cfg(windows)]
fn registered_format(name: &str, slot: &'static OnceLock<u16>) -> u16 {
    *slot.get_or_init(|| {
        let wide: Vec<u16> = name.encode_utf16().chain(std::iter::once(0)).collect();
        unsafe { RegisterClipboardFormatW(wide.as_ptr()) }
    })
}

#[cfg(windows)]
fn clipboard_format_dword(format: u16) -> Option<u32> {
    if format == 0 || unsafe { IsClipboardFormatAvailable(format as u32) } == 0 {
        return None;
    }
    unsafe {
        let memory = GetClipboardData(format as u32);
        if memory.is_null() || GlobalSize(memory) < std::mem::size_of::<u32>() {
            return None;
        }
        let locked = GlobalLock(memory);
        if locked.is_null() {
            return None;
        }
        let value = std::ptr::read_unaligned(locked.cast::<u32>());
        let _ = GlobalUnlock(memory);
        Some(value)
    }
}

#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
fn should_exclude_clipboard(
    explicit_exclusion: bool,
    can_include_in_history: Option<u32>,
    can_upload_to_cloud: Option<u32>,
) -> bool {
    explicit_exclusion || can_include_in_history == Some(0) || can_upload_to_cloud == Some(0)
}

#[cfg(windows)]
fn clip_excluded() -> bool {
    static EXCLUDE_MONITOR: OnceLock<u16> = OnceLock::new();
    static INCLUDE_HISTORY: OnceLock<u16> = OnceLock::new();
    static UPLOAD_CLOUD: OnceLock<u16> = OnceLock::new();
    let explicit = registered_format(
        "ExcludeClipboardContentFromMonitorProcessing",
        &EXCLUDE_MONITOR,
    );
    let history = registered_format("CanIncludeInClipboardHistory", &INCLUDE_HISTORY);
    let cloud = registered_format("CanUploadToCloudClipboard", &UPLOAD_CLOUD);
    should_exclude_clipboard(
        explicit != 0 && unsafe { IsClipboardFormatAvailable(explicit as u32) } != 0,
        clipboard_format_dword(history),
        clipboard_format_dword(cloud),
    )
}

#[cfg(target_os = "linux")]
fn clip_excluded() -> bool {
    x11watch::excluded()
}

#[cfg(not(any(windows, target_os = "linux")))]
fn clip_excluded() -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::should_exclude_clipboard;

    #[test]
    fn clipboard_exclusion_flags_are_fail_closed() {
        assert!(should_exclude_clipboard(true, Some(1), Some(1)));
        assert!(should_exclude_clipboard(false, Some(0), Some(1)));
        assert!(should_exclude_clipboard(false, Some(1), Some(0)));
        assert!(!should_exclude_clipboard(false, Some(1), Some(1)));
        assert!(!should_exclude_clipboard(false, None, None));
    }
}
