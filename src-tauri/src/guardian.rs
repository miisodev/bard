use std::process::Command;
#[cfg(windows)]
use std::sync::atomic::{AtomicBool, Ordering};
#[cfg(windows)]
use std::sync::Once;
#[cfg(windows)]
use std::thread;
#[cfg(windows)]
use std::time::{SystemTime, UNIX_EPOCH};

#[cfg(windows)]
const HEALTH_MS: u32 = 20000;
#[cfg(windows)]
const WAIT_OBJECT_0: u32 = 0;
#[cfg(windows)]
const CTRL_C_EVENT: u32 = 0;
#[cfg(windows)]
const CTRL_BREAK_EVENT: u32 = 1;
#[cfg(windows)]
const DETACHED_PROCESS: u32 = 0x0000_0008;
#[cfg(windows)]
const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;

#[cfg(windows)]
static CONSOLE_INTERRUPTED: AtomicBool = AtomicBool::new(false);
#[cfg(windows)]
static CONSOLE_HANDLER_INSTALLED: Once = Once::new();

// The watchdog waits on a Windows process handle and the console handler is a
// Windows console event; Linux gets its process-watching equivalent in the
// platform lane. Only the marker itself is cross-platform.
#[cfg(windows)]
#[link(name = "kernel32")]
extern "system" {
    fn OpenProcess(access: u32, inherit: i32, pid: u32) -> isize;
    fn WaitForSingleObject(h: isize, ms: u32) -> u32;
    fn CloseHandle(h: isize) -> i32;
}

#[cfg(windows)]
#[link(name = "kernel32")]
extern "system" {
    fn SetConsoleCtrlHandler(
        handler: Option<unsafe extern "system" fn(u32) -> i32>,
        add: i32,
    ) -> i32;
}

/// Where the clean-exit marker lives. State, not data: `paths::state_dir`
/// keeps Windows at `%LOCALAPPDATA%\com.miisodev.bard` and gives Linux the
/// XDG state directory. A machine that resolves neither falls back to the
/// temp directory, exactly as before.
pub fn marker_path() -> std::path::PathBuf {
    crate::paths::state_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("clean-exit")
}

pub fn mark_clean_exit() {
    let _ = std::fs::write(marker_path(), "1");
}

pub fn clear_stale_marker() {
    let _ = std::fs::remove_file(marker_path());
}

/// Treat Ctrl+C as an explicit, clean shutdown even when the UI thread is
/// wedged. The handler itself only flips an atomic; a normal watchdog thread
/// writes the marker and exits, so the console event cannot take down the
/// guardian or leave it believing that an ordinary quit was a crash.
///
/// Windows-only: Ctrl+C arrives as SIGINT on Linux and ends the process by
/// itself, so there is no console event to translate and nothing for the
/// watchdog thread to watch.
pub fn install_console_handler() {
    #[cfg(windows)]
    {
        CONSOLE_HANDLER_INSTALLED.call_once(|| unsafe {
            let _ = SetConsoleCtrlHandler(Some(console_ctrl_handler), 1);
        });

        thread::spawn(|| {
            while !CONSOLE_INTERRUPTED.load(Ordering::Acquire) {
                thread::sleep(std::time::Duration::from_millis(100));
            }
            mark_clean_exit();
            write_console_event();
            std::process::exit(0);
        });
    }
}

#[cfg(windows)]
unsafe extern "system" fn console_ctrl_handler(kind: u32) -> i32 {
    if kind == CTRL_C_EVENT || kind == CTRL_BREAK_EVENT {
        CONSOLE_INTERRUPTED.store(true, Ordering::Release);
        1
    } else {
        0
    }
}

#[cfg(windows)]
fn write_console_event() {
    let directory = crate::paths::logs_dir();
    if std::fs::create_dir_all(&directory).is_err() {
        return;
    }
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or(0);
    let report = format!(
        "event: console_interrupt\ntimestamp_unix_ms: {timestamp}\nprocess_id: {}\n",
        std::process::id()
    );
    let _ = std::fs::write(directory.join("console-interrupt.log"), report);
}

pub fn spawn(exe: &str) {
    let pid = format!("{}", std::process::id());
    let mut command = Command::new(exe);
    command.args(["--guardian", &pid]);

    // A guardian must not share the developer's console. Otherwise Ctrl+C is
    // delivered to both processes and can kill the watchdog at the exact
    // moment it is supposed to decide whether the parent exited cleanly.
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP);
    }

    let _ = command.spawn();
}

/// Watch the parent process and relaunch it when it died without writing the
/// clean-exit marker. The watchdog waits on a Windows process handle; Linux's
/// equivalent is the platform lane's to build.
#[cfg(windows)]
pub fn run() {
    let args = std::env::args().collect::<Vec<_>>();
    let Some(exe) = args.first() else {
        return;
    };
    let Some(pid) = args.get(2).and_then(|s| s.parse::<u32>().ok()) else {
        return;
    };
    let handle = unsafe { OpenProcess(0x0010_0400, 0, pid) };
    if handle == 0 {
        return;
    }
    let status = unsafe { WaitForSingleObject(handle, HEALTH_MS) };
    unsafe {
        CloseHandle(handle);
    }
    if status != WAIT_OBJECT_0 {
        return;
    }
    let marker = marker_path();
    if marker.exists() {
        let _ = std::fs::remove_file(&marker);
        return;
    }
    let mut command = Command::new(exe);
    command.arg("--restarted");
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP);
    }
    let _ = command.spawn();
}

/// The guardian flag is opt-in diagnosis on every platform; with no watchdog
/// to run here yet, the process exits rather than pretending to watch. The
/// marker itself is cross-platform and is written and cleared by the normal
/// startup and shutdown paths.
#[cfg(not(windows))]
pub fn run() {}
