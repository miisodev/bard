//! Bounded, local crash evidence for unhandled Rust panics.
//!
//! The hook deliberately depends only on `std`. It creates a crash directory,
//! keeps a small number of timestamped logs, and treats every recoverable
//! failure as best-effort so a panic handler cannot turn one failure into a
//! second panic.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::panic::{self, AssertUnwindSafe, PanicHookInfo};
use std::path::{Path, PathBuf};
use std::process;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Mutex, Once};
use std::thread;
use std::time::{SystemTime, UNIX_EPOCH};

const FILE_PREFIX: &str = "crash-";
const FILE_SUFFIX: &str = ".log";
const TIMESTAMP_DIGITS: usize = 39;
const PROCESS_ID_DIGITS: usize = 10;
const SEQUENCE_DIGITS: usize = 20;
const UNIQUE_NAME_ATTEMPTS: usize = 16;

/// A crash log selected by [`read_latest_log`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CrashLog {
    pub path: PathBuf,
    pub contents: String,
}

static INSTALL_HOOK: Once = Once::new();
static CRASH_DIRECTORY: Mutex<Option<PathBuf>> = Mutex::new(None);
static IN_HOOK: AtomicBool = AtomicBool::new(false);
static NEXT_SEQUENCE: AtomicU64 = AtomicU64::new(0);

const MAX_LOG_BYTES: usize = 512 * 1024;
const MAX_LOG_FILES: usize = 5;
const MAX_PAYLOAD_BYTES: usize = 16 * 1024;
const MAX_LIFECYCLE_BYTES: u64 = 256 * 1024;
/// How many rotated lifecycle logs to keep alongside the current one.
const MAX_LIFECYCLE_FILES: u32 = 4;
const REPORT_TRUNCATION_MARKER: &str = "\n[truncated]\n";
const READ_TRUNCATION_MARKER: &str = "\n[truncated while reading]\n";
const PAYLOAD_TRUNCATION_MARKER: &str = "...[truncated]";

static LOGGER: LifecycleLogger = LifecycleLogger;

/// A `log` sink that writes into the same lifecycle log the gesture events use.
///
/// The `log` crate is a dependency and the tree calls `log::warn!` in three
/// places, but nothing ever called `set_logger`. Without it the crate's
/// `MAX_LOG_LEVEL` starts at `Off` and every one of those calls is discarded at
/// runtime — not merely invisible in a windowed app, but never executed
/// anywhere. This sink reuses [`record_event`] so the evidence lands where it is
/// already read, and cannot become a second failure because that write is
/// bounded and swallows I/O errors.
struct LifecycleLogger;

impl log::Log for LifecycleLogger {
    fn enabled(&self, metadata: &log::Metadata) -> bool {
        metadata.level() <= log::Level::Warn
    }

    fn log(&self, record: &log::Record) {
        if !self.enabled(record.metadata()) {
            return;
        }
        record_event(&format!(
            "log:{}:{}",
            record.level().as_str().to_lowercase(),
            record.args()
        ));
    }

    fn flush(&self) {}
}

fn install_logger() {
    // `set_logger` fails if something already claimed the sink. Bard installs one
    // thing, so a failure means a future caller arrived first; respect it rather
    // than overwriting whatever is there.
    if log::set_logger(&LOGGER).is_ok() {
        log::set_max_level(log::LevelFilter::Warn);
    }
}

/// Install the process-wide panic hook and use `directory` for crash logs.
///
/// Calling this more than once updates the destination without chaining
/// another hook. The directory is created immediately on a best-effort basis
/// and again when a report is written. This function does not surface I/O
/// errors because the panic hook has no useful recovery path.
pub fn install(directory: &Path) {
    let _ = panic::catch_unwind(AssertUnwindSafe(|| {
        install_inner(directory);
    }));
}

fn install_inner(directory: &Path) {
    // `set_hook` rejects calls made by an already-panicking thread. There is no
    // useful logging work to do in that case, and avoiding the call is safer
    // than relying on `catch_unwind` (which cannot help under panic=abort).
    if thread::panicking() {
        return;
    }

    let directory = directory.to_path_buf();
    let _ = create_crash_directory(&directory);
    set_configured_directory(directory);
    install_logger();

    let _ = panic::catch_unwind(AssertUnwindSafe(|| {
        INSTALL_HOOK.call_once(|| {
            let previous = panic::take_hook();
            panic::set_hook(Box::new(move |info| {
                // A panic while handling a panic must not recurse forever. It
                // is also important under concurrent panics: one process should
                // produce one coherent report rather than several racing ones.
                if IN_HOOK.swap(true, Ordering::AcqRel) {
                    return;
                }

                let _ = panic::catch_unwind(AssertUnwindSafe(|| {
                    if let Some(directory) = configured_directory() {
                        let _ = write_crash_report(&directory, info);
                    }
                }));

                // Preserve the runtime/library hook for debug output and test
                // frameworks, but only after the local evidence is on disk.
                let _ = panic::catch_unwind(AssertUnwindSafe(|| previous(info)));
                IN_HOOK.store(false, Ordering::Release);
            }));
        });
    }));
}

fn set_configured_directory(directory: PathBuf) {
    match CRASH_DIRECTORY.lock() {
        Ok(mut configured) => *configured = Some(directory),
        Err(poisoned) => *poisoned.into_inner() = Some(directory),
    }
}

fn configured_directory() -> Option<PathBuf> {
    match CRASH_DIRECTORY.lock() {
        Ok(configured) => configured.clone(),
        Err(poisoned) => poisoned.into_inner().clone(),
    }
}

/// Record a small, secret-free lifecycle breadcrumb. This is deliberately
/// separate from panic reports: it lets support distinguish a clean quit, a
/// console interrupt, a stuck audio start, and a native inference failure
/// without retaining transcripts, paths, or provider responses.
pub fn record_event(event: &str) {
    let Some(directory) = configured_directory() else {
        return;
    };
    if create_crash_directory(&directory).is_err() {
        return;
    }
    let path = directory.join("lifecycle.log");
    let line = format!(
        "{}\tpid={}\t{}\n",
        unix_timestamp_millis(),
        process::id(),
        event.replace(['\r', '\n', '\t'], " ")
    );
    if fs::metadata(&path)
        .map(|metadata| metadata.len())
        .unwrap_or(0)
        >= MAX_LIFECYCLE_BYTES
    {
        // Rotated, not destroyed. The previous behaviour overwrote the file with
        // a single `[lifecycle log truncated]` line, which is indistinguishable
        // from a dead app — so on any long-running install the one piece of
        // evidence that could explain an input or monitor failure was erased by
        // the act of logging. Rotation keeps the history, bounded the same way
        // crash logs already are.
        rotate_lifecycle_log(&path);
    }
    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
        let _ = file.write_all(line.as_bytes());
    }
}

/// Move `lifecycle.log` aside and start a fresh one, keeping a bounded set.
///
/// `prune_logs` already does this for crash reports, so the naming and the
/// retention bound match: `lifecycle.log` is always the current file and
/// `lifecycle-<n>.log` are older ones, newest first.
fn rotate_lifecycle_log(path: &Path) {
    let Some(parent) = path.parent() else {
        let _ = fs::write(path, "[lifecycle log truncated]\n");
        return;
    };

    // Shift the numbered files up by one, oldest falling off the end.
    for index in (1..MAX_LIFECYCLE_FILES).rev() {
        let from = numbered_lifecycle_path(parent, index - 1);
        let to = numbered_lifecycle_path(parent, index);
        if from.exists() {
            let _ = fs::rename(&from, &to);
        }
    }

    let first = numbered_lifecycle_path(parent, 0);
    // Could not keep the old contents, so do not pretend the file is bounded:
    // drop it and note that, which is the honest state.
    if path.exists() && fs::rename(path, &first).is_err() {
        let _ = fs::write(path, "[lifecycle log truncated]\n");
        return;
    }

    // Bound the set the same way crash logs are bounded.
    let mut kept = 0;
    for index in 0..MAX_LIFECYCLE_FILES {
        let candidate = numbered_lifecycle_path(parent, index);
        if !candidate.exists() {
            continue;
        }
        kept += 1;
        if kept > MAX_LIFECYCLE_FILES {
            let _ = fs::remove_file(&candidate);
        }
    }
}

fn numbered_lifecycle_path(directory: &Path, index: u32) -> PathBuf {
    directory.join(format!("lifecycle-{index}.log"))
}

/// The tail of the newest lifecycle log, for the diagnostic report: the last
/// `max_lines` lines, oldest first. A missing file is an empty string; a file
/// that cannot be read says so rather than looking like "nothing happened".
pub fn lifecycle_tail(directory: &Path, max_lines: usize) -> String {
    let path = directory.join("lifecycle.log");
    match read_bounded(&path) {
        Ok(contents) => {
            let lines: Vec<&str> = contents.lines().collect();
            let start = lines.len().saturating_sub(max_lines);
            let mut tail = lines[start..].join("\n");
            if !tail.is_empty() {
                tail.push('\n');
            }
            tail
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => String::new(),
        Err(error) => format!("[lifecycle log unreadable: {error}]\n"),
    }
}

fn create_crash_directory(directory: &Path) -> io::Result<()> {
    fs::create_dir_all(directory)
}

/// Read the newest recognized Bard crash log from `directory`.
///
/// A missing directory or a directory with no crash logs is `Ok(None)`. File
/// and decoding errors are returned to the caller. The returned string is
/// bounded even if the file was replaced by an oversized external file.
pub fn read_latest_log(directory: &Path) -> io::Result<Option<CrashLog>> {
    for log in collect_log_files(directory)? {
        match read_bounded(&log.path) {
            Ok(contents) => {
                return Ok(Some(CrashLog {
                    path: log.path,
                    contents,
                }))
            }
            // A settings reader can race antivirus cleanup or another writer.
            // Try the next candidate instead of turning that race into an
            // error for the whole command.
            Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
            Err(error) => return Err(error),
        }
    }

    Ok(None)
}

fn write_crash_report(directory: &Path, info: &PanicHookInfo<'_>) -> io::Result<PathBuf> {
    create_crash_directory(directory)?;

    // Pruning is best effort. A locked or antivirus-held old file must not
    // suppress the report for the panic that is happening right now. The
    // normal path still keeps the retention bound; an exceptional cleanup
    // failure may leave one extra report rather than losing current evidence.
    let _ = prune_logs(directory, MAX_LOG_FILES.saturating_sub(1));

    let report = format_report(info);
    let path = write_new_log(directory, report)?;

    // The pre-write prune made this redundant in the single-process case, but
    // repeating it handles another Bard process that started concurrently.
    let _ = prune_logs(directory, MAX_LOG_FILES);
    Ok(path)
}

fn format_report(info: &PanicHookInfo<'_>) -> String {
    let timestamp = unix_timestamp_millis();
    let current_thread = thread::current();
    let thread_name = current_thread.name().unwrap_or("<unnamed>");
    let thread_id = current_thread.id();
    let payload = format_payload(info.payload_as_str());
    let location = match info.location() {
        Some(location) => format!(
            "{}:{}:{}",
            location.file(),
            location.line(),
            location.column()
        ),
        None => "<unknown>".to_string(),
    };
    let backtrace = std::backtrace::Backtrace::force_capture().to_string();
    let panic_strategy = if cfg!(panic = "abort") {
        "abort"
    } else {
        "unwind"
    };

    let mut report = String::new();
    report.push_str("bard crash report\n");
    report.push_str("timestamp_unix_ms: ");
    report.push_str(&timestamp.to_string());
    report.push('\n');
    report.push_str("process_id: ");
    report.push_str(&process::id().to_string());
    report.push('\n');
    report.push_str("panic_strategy: ");
    report.push_str(panic_strategy);
    report.push('\n');
    report.push_str("app_version: ");
    report.push_str(env!("CARGO_PKG_VERSION"));
    report.push('\n');
    report.push_str("executable: ");
    report.push_str(
        &std::env::current_exe()
            .map(|path| path.to_string_lossy().to_string())
            .unwrap_or_else(|_| "<unknown>".into()),
    );
    report.push('\n');
    report.push_str("thread_name: ");
    report.push_str(&format!("{thread_name:?}"));
    report.push('\n');
    report.push_str("thread_id: ");
    report.push_str(&format!("{thread_id:?}"));
    report.push('\n');
    report.push_str("panic_location: ");
    report.push_str(&location);
    report.push('\n');
    report.push_str("panic_payload: ");
    report.push_str(&payload);
    report.push_str("\n\nbacktrace (best effort):\n");
    report.push_str(&backtrace);
    if !report.ends_with('\n') {
        report.push('\n');
    }

    truncate_report(report)
}

fn format_payload(payload: Option<&str>) -> String {
    let Some(payload) = payload else {
        return "<non-string panic payload>".to_string();
    };

    let mut output = String::from("\"");
    let content_limit = MAX_PAYLOAD_BYTES
        .saturating_sub(2)
        .saturating_sub(PAYLOAD_TRUNCATION_MARKER.len());
    let mut truncated = false;

    for character in payload.chars() {
        if output.len() >= content_limit {
            truncated = true;
            break;
        }

        let before_character = output.len();
        match character {
            '"' => output.push_str("\\\""),
            '\\' => output.push_str("\\\\"),
            '\n' => output.push_str("\\n"),
            '\r' => output.push_str("\\r"),
            '\t' => output.push_str("\\t"),
            control if control.is_control() => {
                output.push_str("\\u{");
                output.push_str(&format!("{:x}", control as u32));
                output.push('}');
            }
            other => output.push(other),
        }

        if output.len() > content_limit {
            output.truncate(before_character);
            truncated = true;
            break;
        }
    }

    output.push('"');
    if truncated {
        output.push_str(PAYLOAD_TRUNCATION_MARKER);
    }
    output
}

fn truncate_report(mut report: String) -> String {
    if report.len() <= MAX_LOG_BYTES {
        return report;
    }

    let content_limit = MAX_LOG_BYTES.saturating_sub(REPORT_TRUNCATION_MARKER.len());
    let boundary = floor_char_boundary(&report, content_limit);
    report.truncate(boundary);
    report.push_str(REPORT_TRUNCATION_MARKER);
    report
}

fn floor_char_boundary(value: &str, mut index: usize) -> usize {
    if index >= value.len() {
        return value.len();
    }
    while index > 0 && !value.is_char_boundary(index) {
        index -= 1;
    }
    index
}

fn unix_timestamp_millis() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis())
        .unwrap_or(0)
}

fn log_file_name(timestamp: u128, process_id: u32, sequence: u64) -> String {
    format!("{FILE_PREFIX}{timestamp:039}-{process_id:010}-{sequence:020}{FILE_SUFFIX}")
}

fn write_new_log(directory: &Path, report: String) -> io::Result<PathBuf> {
    let report = truncate_report(report);
    let process_id = process::id();

    for _ in 0..UNIQUE_NAME_ATTEMPTS {
        let sequence = NEXT_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let path = directory.join(log_file_name(unix_timestamp_millis(), process_id, sequence));
        let mut file = match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(file) => file,
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        };

        // Keep a partial file when the OS rejects a later write: bounded
        // partial evidence is more useful than silently deleting it.
        file.write_all(report.as_bytes())?;
        let _ = file.sync_all();
        drop(file);
        return Ok(path);
    }

    Err(io::Error::new(
        io::ErrorKind::AlreadyExists,
        "could not allocate a unique crash log filename",
    ))
}

#[derive(Debug)]
struct LogFile {
    path: PathBuf,
    modified: Option<SystemTime>,
    timestamp: u128,
    process_id: u32,
    sequence: u64,
}

fn collect_log_files(directory: &Path) -> io::Result<Vec<LogFile>> {
    let entries = match fs::read_dir(directory) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error),
    };
    let mut logs = Vec::new();

    for entry in entries {
        let entry = match entry {
            Ok(entry) => entry,
            Err(_) => continue,
        };
        let file_type = match entry.file_type() {
            Ok(file_type) => file_type,
            Err(_) => continue,
        };
        if !file_type.is_file() {
            continue;
        }
        let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
            continue;
        };
        let Some((timestamp, process_id, sequence)) = parse_log_file_name(&name) else {
            continue;
        };
        let modified = entry
            .metadata()
            .ok()
            .and_then(|metadata| metadata.modified().ok());
        logs.push(LogFile {
            path: entry.path(),
            modified,
            timestamp,
            process_id,
            sequence,
        });
    }

    // Modification time answers "which file was written last" even if the wall
    // clock moved backwards. Strict names and the fixed-width fields provide
    // deterministic ordering when timestamps or filesystem clocks tie.
    logs.sort_by(|left, right| {
        right
            .modified
            .cmp(&left.modified)
            .then_with(|| right.timestamp.cmp(&left.timestamp))
            .then_with(|| right.process_id.cmp(&left.process_id))
            .then_with(|| right.sequence.cmp(&left.sequence))
    });
    Ok(logs)
}

fn parse_log_file_name(name: &str) -> Option<(u128, u32, u64)> {
    let value = name.strip_prefix(FILE_PREFIX)?.strip_suffix(FILE_SUFFIX)?;
    let expected_length = TIMESTAMP_DIGITS + 1 + PROCESS_ID_DIGITS + 1 + SEQUENCE_DIGITS;
    if value.len() != expected_length {
        return None;
    }

    let bytes = value.as_bytes();
    let timestamp_end = TIMESTAMP_DIGITS;
    let process_id_start = timestamp_end + 1;
    let process_id_end = process_id_start + PROCESS_ID_DIGITS;
    let sequence_start = process_id_end + 1;
    let sequence_end = sequence_start + SEQUENCE_DIGITS;
    if bytes[timestamp_end] != b'-' || bytes[process_id_end] != b'-' {
        return None;
    }

    let timestamp = parse_decimal(&bytes[..timestamp_end])?;
    let process_id = parse_decimal(&bytes[process_id_start..process_id_end])?;
    let sequence = parse_decimal(&bytes[sequence_start..sequence_end])?;
    Some((
        timestamp,
        u32::try_from(process_id).ok()?,
        u64::try_from(sequence).ok()?,
    ))
}

fn parse_decimal(bytes: &[u8]) -> Option<u128> {
    if bytes.is_empty() {
        return None;
    }

    let mut value = 0_u128;
    for &byte in bytes {
        if !byte.is_ascii_digit() {
            return None;
        }
        value = value.checked_mul(10)?.checked_add((byte - b'0') as u128)?;
    }
    Some(value)
}

fn prune_logs(directory: &Path, maximum: usize) -> io::Result<usize> {
    let mut logs = collect_log_files(directory)?;
    if logs.len() <= maximum {
        return Ok(0);
    }

    let stale = logs.split_off(maximum);
    let mut removed = 0;
    for log in stale {
        match fs::remove_file(&log.path) {
            Ok(()) => removed += 1,
            Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
            Err(error) => return Err(error),
        }
    }
    Ok(removed)
}

fn read_bounded(path: &Path) -> io::Result<String> {
    let file = File::open(path)?;
    let mut bytes = Vec::with_capacity(MAX_LOG_BYTES.saturating_add(1));
    let mut reader = file.take((MAX_LOG_BYTES.saturating_add(1)) as u64);
    reader.read_to_end(&mut bytes)?;

    let should_truncate = bytes.len() > MAX_LOG_BYTES;
    if should_truncate {
        bytes.truncate(MAX_LOG_BYTES.saturating_sub(READ_TRUNCATION_MARKER.len()));
    }

    match String::from_utf8(bytes) {
        Ok(mut contents) => {
            if should_truncate {
                contents.push_str(READ_TRUNCATION_MARKER);
            }
            Ok(contents)
        }
        Err(error) if should_truncate && error.utf8_error().error_len().is_none() => {
            // Truncation may have split the final UTF-8 code point. Preserve
            // the valid prefix and make the read-side truncation explicit.
            let valid_length = error.utf8_error().valid_up_to();
            let mut valid_bytes = error.into_bytes();
            valid_bytes.truncate(valid_length);
            let mut contents = String::from_utf8(valid_bytes).map_err(|_| {
                io::Error::new(io::ErrorKind::InvalidData, "crash log is not valid UTF-8")
            })?;
            contents.push_str(READ_TRUNCATION_MARKER);
            Ok(contents)
        }
        Err(_) => Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "crash log is not valid UTF-8",
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    static NEXT_TEST_DIRECTORY: AtomicU64 = AtomicU64::new(0);

    struct TestDirectory(PathBuf);

    impl TestDirectory {
        fn new(label: &str) -> Self {
            let sequence = NEXT_TEST_DIRECTORY.fetch_add(1, Ordering::Relaxed);
            let timestamp = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|elapsed| elapsed.as_nanos())
                .unwrap_or(0);
            let path = std::env::temp_dir().join(format!(
                "bard-crash-{label}-{}-{timestamp}-{sequence}",
                process::id()
            ));
            let _ = fs::remove_dir_all(&path);
            Self(path)
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TestDirectory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn creates_nested_crash_directory() {
        let root = TestDirectory::new("path");
        let nested = root.path().join("nested").join("crash-logs");

        create_crash_directory(&nested).unwrap();

        assert!(nested.is_dir());
    }

    #[test]
    fn rotation_keeps_only_the_newest_bounded_set() {
        let root = TestDirectory::new("rotation");
        create_crash_directory(root.path()).unwrap();
        let count = MAX_LOG_FILES + 3;

        for index in 0..count {
            let timestamp = (index + 1) as u128;
            let sequence = index as u64;
            let path = root
                .path()
                .join(log_file_name(timestamp, process::id(), sequence));
            let mut file = File::create(path).unwrap();
            file.write_all(format!("crash {index}").as_bytes()).unwrap();
        }

        let removed = prune_logs(root.path(), MAX_LOG_FILES).unwrap();
        let logs = collect_log_files(root.path()).unwrap();

        assert_eq!(removed, 3);
        assert_eq!(logs.len(), MAX_LOG_FILES);
        assert_eq!(logs[0].timestamp, count as u128);
        assert_eq!(
            logs[MAX_LOG_FILES - 1].timestamp,
            (count - MAX_LOG_FILES + 1) as u128
        );
    }

    #[test]
    fn payload_formatting_is_single_line_and_bounded() {
        assert_eq!(format_payload(Some("boom")), "\"boom\"");
        assert_eq!(format_payload(None), "<non-string panic payload>");
        assert_eq!(
            format_payload(Some("first\nsecond \"quoted\"")),
            "\"first\\nsecond \\\"quoted\\\"\""
        );

        let oversized = "x".repeat(MAX_PAYLOAD_BYTES + 257);
        let formatted = format_payload(Some(&oversized));
        assert!(formatted.len() <= MAX_PAYLOAD_BYTES);
        assert!(formatted.ends_with(PAYLOAD_TRUNCATION_MARKER));
    }

    #[test]
    fn latest_log_is_readable_and_remains_bounded() {
        let root = TestDirectory::new("latest");
        create_crash_directory(root.path()).unwrap();

        let _ = write_new_log(root.path(), "first crash\n".to_string()).unwrap();
        let latest_path = write_new_log(root.path(), "x".repeat(MAX_LOG_BYTES + 1024)).unwrap();

        let latest = read_latest_log(root.path()).unwrap().unwrap();
        assert_eq!(latest.path, latest_path);
        assert!(latest.contents.len() <= MAX_LOG_BYTES);
        assert!(latest.contents.ends_with(REPORT_TRUNCATION_MARKER));
        assert!(latest_path.metadata().unwrap().len() <= MAX_LOG_BYTES as u64);
    }

    #[test]
    fn lifecycle_tail_keeps_only_the_newest_lines() {
        let root = TestDirectory::new("lifecycle-tail");
        create_crash_directory(root.path()).unwrap();
        let mut file = File::create(root.path().join("lifecycle.log")).unwrap();
        for index in 0..10 {
            writeln!(file, "line {index}").unwrap();
        }
        drop(file);

        assert_eq!(lifecycle_tail(root.path(), 3), "line 7\nline 8\nline 9\n");
        assert_eq!(lifecycle_tail(root.path(), 100).lines().count(), 10);
        // A directory that never recorded anything answers with nothing
        // rather than a fabricated line.
        assert_eq!(lifecycle_tail(&root.path().join("missing"), 3), "");
    }
}
