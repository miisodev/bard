use std::process::Command;

const HEALTH_MS: u32 = 20000;
const WAIT_OBJECT_0: u32 = 0;

#[link(name = "kernel32")]
extern "system" {
    fn OpenProcess(access: u32, inherit: i32, pid: u32) -> isize;
    fn WaitForSingleObject(h: isize, ms: u32) -> u32;
    fn CloseHandle(h: isize) -> i32;
}

pub fn marker_path() -> std::path::PathBuf {
    let root = std::env::var("APPDATA")
        .map(|p| format!("{p}\\com.miisodev.bard"))
        .unwrap_or(std::env::temp_dir().to_string_lossy().to_string());
    std::path::Path::new(&root).join("clean-exit")
}

pub fn mark_clean_exit() {
    let _ = std::fs::write(&marker_path(), "1");
}

pub fn spawn(exe: &str) {
    let pid = format!("{}", std::process::id());
    let _ = Command::new(exe).args(["--guardian", &pid]).spawn();
}

pub fn run() {
    let args = std::env::args().collect::<Vec<_>>();
    let Some(exe) = args.get(0) else {
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
    let _ = Command::new(exe).arg("--restarted").spawn();
}