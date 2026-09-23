use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Once;
use std::thread;
use std::time::{SystemTime, UNIX_EPOCH};

const HEALTH_MS: u32 = 20000;
const WAIT_OBJECT_0: u32 = 0;
const CTRL_C_EVENT: u32 = 0;
const CTRL_BREAK_EVENT: u32 = 1;
const DETACHED_PROCESS: u32 = 0x0000_0008;
const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;

static CONSOLE_INTERRUPTED: AtomicBool = AtomicBool::new(false);
static CONSOLE_HANDLER_INSTALLED: Once = Once::new();

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

pub fn marker_path() -> std::path::PathBuf {
    let root = std::env::var("LOCALAPPDATA")
        .map(|p| format!("{p}\\com.miisodev.bard"))
        .unwrap_or(std::env::temp_dir().to_string_lossy().to_string());
    std::path::Path::new(&root).join("clean-exit")
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
pub fn install_console_handler() {
    #[cfg(windows)]
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

#[cfg(windows)]
unsafe extern "system" fn console_ctrl_handler(kind: u32) -> i32 {
    if kind == CTRL_C_EVENT || kind == CTRL_BREAK_EVENT {
        CONSOLE_INTERRUPTED.store(true, Ordering::Release);
        1
    } else {
        0
    }
}

fn write_console_event() {
    let Some(base) = std::env::var_os("LOCALAPPDATA") else {
        return;
    };
    let directory = std::path::Path::new(&base)
        .join("com.miisodev.bard")
        .join("logs");
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
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP);
    }
    let _ = command.spawn();
}
