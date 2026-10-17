//! `mods/loader.log` (truncated each launch) plus an optional console window.

use std::fs::File;
use std::io::Write;
use std::sync::Mutex;
use std::time::Instant;

use hd_sdk::LogLevel;

struct Logger {
    file: Option<File>,
    console: bool,
    max_level: LogLevel,
    start: Instant,
}

static LOGGER: Mutex<Option<Logger>> = Mutex::new(None);

pub fn init(path: &std::path::Path, console: bool, max_level: LogLevel) {
    if console {
        unsafe {
            windows_sys::Win32::System::Console::AllocConsole();
        }
    }
    let file = File::create(path).ok();
    *LOGGER.lock().unwrap() = Some(Logger { file, console, max_level, start: Instant::now() });
}

pub fn write(level: LogLevel, module: &str, msg: &str) {
    let mut guard = match LOGGER.lock() {
        Ok(g) => g,
        Err(p) => p.into_inner(),
    };
    let Some(l) = guard.as_mut() else { return };
    if level > l.max_level {
        return;
    }
    let tag = match level {
        LogLevel::Error => "ERROR",
        LogLevel::Warn => "WARN ",
        LogLevel::Info => "INFO ",
        LogLevel::Debug => "DEBUG",
    };
    let line = format!("[{:>9.3}] {tag} [{module}] {msg}\r\n", l.start.elapsed().as_secs_f64());
    if let Some(f) = l.file.as_mut() {
        let _ = f.write_all(line.as_bytes());
        let _ = f.flush();
    }
    if l.console {
        // The process has no CRT-attached stdout; write to the console directly.
        let _ = std::io::stdout().write_all(line.as_bytes());
    }
}

macro_rules! log_error { ($($t:tt)*) => { $crate::log::write(hd_sdk::LogLevel::Error, "loader", &format!($($t)*)) } }
macro_rules! log_warn { ($($t:tt)*) => { $crate::log::write(hd_sdk::LogLevel::Warn, "loader", &format!($($t)*)) } }
macro_rules! log_info { ($($t:tt)*) => { $crate::log::write(hd_sdk::LogLevel::Info, "loader", &format!($($t)*)) } }
pub(crate) use {log_error, log_info, log_warn};
