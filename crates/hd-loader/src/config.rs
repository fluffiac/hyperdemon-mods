//! `mods/loader.ini`: tiny `key = value` file, created with defaults on first run.

use std::path::Path;

use hd_sdk::LogLevel;

pub struct Config {
    /// Open a console window that mirrors the log.
    pub console: bool,
    pub log_level: LogLevel,
    /// Mod file stems to skip (comma separated in the file).
    pub disabled: Vec<String>,
}

const DEFAULT: &str = "\
; HyperDemon mod loader settings
; Mods are the .dll files in this folder. Files starting with '_' are skipped.

; Open a console window mirroring loader.log
console = false

; error | warn | info | debug
log_level = info

; Comma separated mod names (file name without .dll) to skip
disabled =
";

impl Config {
    pub fn load(path: &Path) -> Config {
        let text = match std::fs::read_to_string(path) {
            Ok(t) => t,
            Err(_) => {
                let _ = std::fs::write(path, DEFAULT);
                DEFAULT.to_owned()
            }
        };
        let mut cfg = Config { console: false, log_level: LogLevel::Info, disabled: Vec::new() };
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with(';') || line.starts_with('#') {
                continue;
            }
            let Some((k, v)) = line.split_once('=') else { continue };
            let v = v.trim();
            match k.trim() {
                "console" => cfg.console = matches!(v, "true" | "1" | "yes"),
                "log_level" => {
                    cfg.log_level = match v {
                        "error" => LogLevel::Error,
                        "warn" => LogLevel::Warn,
                        "debug" => LogLevel::Debug,
                        _ => LogLevel::Info,
                    }
                }
                "disabled" => {
                    cfg.disabled =
                        v.split(',').map(|s| s.trim().to_lowercase()).filter(|s| !s.is_empty()).collect()
                }
                _ => {}
            }
        }
        cfg
    }
}
