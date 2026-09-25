//! Best-effort diagnostic log written to `<config_dir>/mfp/mfp.log`.
//!
//! Background threads (e.g. MPRIS) must never write to the terminal because
//! that would corrupt a full-screen UI. They append here instead. Every
//! failure is swallowed: logging can never affect playback.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// Default log location (`<config_dir>/mfp/mfp.log`), if the config dir resolves.
fn default_log_path() -> Option<PathBuf> {
    Some(dirs::config_dir()?.join("mfp").join("mfp.log"))
}

/// Appends `message` as one line to the default log file. Never fails or panics.
pub fn log(message: &str) {
    if let Some(path) = default_log_path() {
        log_to(&path, message);
    }
}

/// Appends `<unix-seconds> <message>` to `path`, creating parent dirs.
/// Errors are ignored (best effort).
pub fn log_to(path: &Path, message: &str) {
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(file, "{} {}", secs, message.replace('\n', " "));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn appends_lines_and_creates_parent_dirs() {
        let dir = std::env::temp_dir().join(format!("mfp-log-test-{}", std::process::id()));
        let path = dir.join("nested").join("mfp.log");
        let _ = fs::remove_dir_all(&dir);

        log_to(&path, "first");
        log_to(&path, "second\nline");

        let content = fs::read_to_string(&path).unwrap();
        let lines: Vec<&str> = content.lines().collect();
        assert_eq!(lines.len(), 2);
        assert!(lines[0].ends_with(" first"));
        assert!(lines[1].ends_with(" second line"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn unwritable_path_is_silent() {
        // A directory path cannot be opened as a file; must not panic.
        log_to(&std::env::temp_dir(), "ignored");
    }
}
