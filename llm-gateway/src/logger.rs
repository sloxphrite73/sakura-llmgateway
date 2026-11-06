//! Append-only runtime log.
//!
//! A double-clicked console window is the only place the gateway's diagnostics
//! normally go, so once it closes (or scrolls) the evidence is gone — which made
//! remote debugging of "it hung for 25 minutes" impossible. `log()` prints to the
//! terminal AND appends to a file next to the config (falling back to
//! %LOCALAPPDATA%), so the operator can always hand over the record.

use std::io::Write;
use std::sync::{Mutex, OnceLock};

static LOG: OnceLock<Mutex<Option<std::fs::File>>> = OnceLock::new();

/// Open (append) the runtime log. Called once at startup; a failure is not fatal.
pub fn init(path: std::path::PathBuf) {
    let file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .ok();
    let opened = file.is_some();
    let _ = LOG.set(Mutex::new(file));
    if opened {
        eprintln!("[gateway] log file:   {}", path.display());
    } else {
        eprintln!(
            "[gateway] log file:   unavailable ({}) — terminal output only",
            path.display()
        );
    }
}

fn stamp() -> String {
    let ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    format!("{ms}")
}

/// Print to the terminal and append the same line to the log file.
pub fn log(msg: &str) {
    eprintln!("{msg}");
    if let Some(m) = LOG.get() {
        if let Ok(mut guard) = m.lock() {
            if let Some(f) = guard.as_mut() {
                let _ = writeln!(f, "{} {}", stamp(), msg);
                let _ = f.flush();
            }
        }
    }
}

/// `format!` convenience wrapper.
#[macro_export]
macro_rules! rlog {
    ($($arg:tt)*) => {
        $crate::logger::log(&format!($($arg)*))
    };
}
