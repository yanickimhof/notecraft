//! A small local log file for warnings, errors and crashes. Nothing leaves the computer.
//!
//! Lines go to `<app data>/NoteCraft/logs/notecraft.log` (next to the search indexes, outside the
//! notes folder); the file rotates at 1 MB and three old files are kept. Writing never fails loudly:
//! a log that cannot be written must not break the app.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;

const MAX_BYTES: u64 = 1_000_000;
const KEEP: usize = 3;

static LOCK: Mutex<()> = Mutex::new(());

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    Info,
    Warn,
    Error,
}

impl Level {
    fn label(self) -> &'static str {
        match self {
            Level::Info => "INFO",
            Level::Warn => "WARN",
            Level::Error => "ERROR",
        }
    }
}

/// Folder of the log files (`NOTECRAFT_LOG_DIR` overrides it).
pub fn log_dir() -> PathBuf {
    if let Some(d) = std::env::var_os("NOTECRAFT_LOG_DIR") {
        return PathBuf::from(d);
    }
    let indexes = crate::sync::index_base_dir();
    indexes.parent().map(|p| p.join("logs")).unwrap_or_else(|| indexes.join("logs"))
}

/// The current log file.
pub fn log_file() -> PathBuf {
    log_dir().join("notecraft.log")
}

fn rotate(file: &std::path::Path) {
    let name = |n: usize| file.with_extension(format!("log.{n}"));
    let _ = fs::remove_file(name(KEEP));
    for n in (1..KEEP).rev() {
        let _ = fs::rename(name(n), name(n + 1));
    }
    let _ = fs::rename(file, name(1));
}

/// Append one line to the log.
pub fn write(level: Level, message: &str) {
    let Ok(_guard) = LOCK.lock() else { return };
    let file = log_file();
    if let Some(dir) = file.parent() {
        if fs::create_dir_all(dir).is_err() {
            return;
        }
    }
    if fs::metadata(&file).map(|m| m.len() > MAX_BYTES).unwrap_or(false) {
        rotate(&file);
    }
    let line = format!("{} {:<5} {}\n", crate::now_string(), level.label(), message.replace('\n', " | "));
    if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(&file) {
        let _ = f.write_all(line.as_bytes());
    }
}

pub fn info(message: &str) {
    write(Level::Info, message);
}

pub fn warn(message: &str) {
    write(Level::Warn, message);
}

pub fn error(message: &str) {
    write(Level::Error, message);
}

/// Record panics (with their location) in the log, then run the default handler.
pub fn install_panic_hook() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        error(&format!("panic: {info}"));
        previous(info);
    }));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lines_are_appended_and_the_file_rotates() {
        let dir = std::env::temp_dir().join(format!("nc-log-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        std::env::set_var("NOTECRAFT_LOG_DIR", &dir);
        warn("first\nsecond line");
        error("boom");
        let text = fs::read_to_string(dir.join("notecraft.log")).unwrap();
        assert!(text.contains("WARN  first | second line"), "{text}");
        assert!(text.contains("ERROR boom"), "{text}");
        // above the limit, the next write starts a fresh file and keeps the old one
        fs::write(dir.join("notecraft.log"), vec![b'x'; (MAX_BYTES + 1) as usize]).unwrap();
        info("after rotation");
        assert!(dir.join("notecraft.log.1").exists());
        let fresh = fs::read_to_string(dir.join("notecraft.log")).unwrap();
        assert!(fresh.contains("after rotation") && fresh.len() < 200, "{}", fresh.len());
        std::env::remove_var("NOTECRAFT_LOG_DIR");
        let _ = fs::remove_dir_all(&dir);
    }
}
