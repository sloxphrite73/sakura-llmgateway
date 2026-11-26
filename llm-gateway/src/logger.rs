//! Append-only runtime log.
//!
//! A double-clicked console window is the only place the gateway's diagnostics
//! normally go, so once it closes (or scrolls) the evidence is gone — which
//! made remote debugging of "it hung for 25 minutes" impossible. `log()`
//! mirrors every line to the terminal AND appends it to a file next to the
//! config (falling back to %LOCALAPPDATA%), so the operator can always hand
//! over the record.
//!
//! The sender side never does I/O: a queued line is one `try_send` away
//! (sub-microsecond). The terminal write and the per-line file flush live on
//! one dedicated writer thread. Callers include `state.rs::mark_*`, which log
//! while holding the pool mutex — a conhost terminal write costs up to
//! milliseconds there, and every request's `select_strategy` (which shares
//! that lock) serialized behind it during 429-storm rotations. Same class as
//! the BPE fix: never block I/O on a lock holder or a tokio worker
//! (bug-ledger BUG-16). When the writer falls behind and the bounded queue
//! fills, lines are DROPPED (bounded memory, never a stalled caller) and the
//! loss is reported on the next line that gets through.

use std::io::Write;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender};
use std::sync::OnceLock;

/// Queued log lines. 1024 ≈ 100 KB of messages: far beyond anything real
/// traffic produces, small enough that a stalled disk never grows the heap.
const QUEUE_CAP: usize = 1024;

/// Rotate at 16 MB, keep 3 rotated copies (`<log>.1/.2/.3`): the long-running
/// Android service writes one [req] line per streamed request, so an
/// append-only file grows without bound (bug-ledger 九-7) — 4 files × 16 MB
/// caps the footprint at 64 MB, no knob to babysit.
const LOG_ROTATE_BYTES: u64 = 16 * 1024 * 1024;
const LOG_ROTATE_KEEP: usize = 3;

static LOG: OnceLock<SyncSender<Line>> = OnceLock::new();

/// Lines dropped because the writer queue was full, reported on the next
/// write (visible loss, never silent).
static DROPPED: AtomicUsize = AtomicUsize::new(0);

/// One queued line: the raw message (the terminal mirror prints it unstamped,
/// exactly as the console always looked) + the enqueue-time epoch-ms stamp
/// (the file line keeps meaning "when it happened" even when the writer runs
/// behind — a write-time stamp would drift during disk stalls).
struct Line {
    ms: u64,
    msg: String,
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Open (append) the runtime log and start its writer thread. Called once at
/// startup; a failure is not fatal. If the writer thread cannot spawn at all,
/// `LOG` stays unset and `log()` degrades to terminal-only — the old
/// pre-init behavior, never a dead queue.
pub fn init(path: std::path::PathBuf) {
    let file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .ok();
    if file.is_some() {
        eprintln!("[gateway] log file:   {}", path.display());
    } else {
        eprintln!(
            "[gateway] log file:   unavailable ({}) — terminal output only",
            path.display()
        );
    }
    let (tx, rx) = mpsc::sync_channel::<Line>(QUEUE_CAP);
    let spawned = std::thread::Builder::new()
        .name("sakura-log".to_string())
        .spawn(move || run_writer(rx, path, file))
        .is_ok();
    if spawned {
        let _ = LOG.set(tx);
    }
}

/// The one thread that touches the terminal and the log file. Single FIFO
/// consumer → line order is preserved. Every line is flushed before the next
/// is taken (the durability that motivated the file mirror is unchanged —
/// only the payer moved). A full queue's drop count leads the next written
/// line, so disk stalls are visible in the record itself. At LOG_ROTATE_BYTES
/// the file rotates (bug-ledger 九-7): append-only otherwise grows without
/// bound under the one-[req]-line-per-request volume.
fn run_writer(
    rx: Receiver<Line>,
    path: std::path::PathBuf,
    mut file: Option<std::fs::File>,
) {
    // Bytes in the current file so far (approximate: the stamp/separators add
    // a fixed slack per line — rotation tolerances are MBs, not bytes). A
    // restart into an already-oversized file rotates on the first line.
    let mut written = file
        .as_ref()
        .and_then(|f| f.metadata().ok())
        .map(|m| m.len())
        .unwrap_or(0);
    for line in rx {
        eprintln!("{}", line.msg);
        let mut wrote = false;
        if let Some(f) = file.as_mut() {
            let dropped = DROPPED.swap(0, Ordering::Relaxed);
            if dropped > 0 {
                let _ = writeln!(
                    f,
                    "{} [log] WARNING: {} log lines dropped (writer queue full)",
                    now_ms(),
                    dropped
                );
            }
            if writeln!(f, "{} {}", line.ms, line.msg).is_ok() {
                wrote = true;
            }
            let _ = f.flush();
        }
        written += line.msg.len() as u64 + 32;
        if wrote && written >= LOG_ROTATE_BYTES {
            file = rotate(&path);
            written = 0;
        }
    }
}

/// Shift the rotated copies down (`.2` → `.3`, `.1` → `.2`, current → `.1`,
/// the oldest falls off) and reopen a fresh file at `path`. Best-effort: a
/// rename that fails leaves the older layout; a reopen failure degrades the
/// writer to terminal-only (the queue keeps draining, lines still mirror).
fn rotate(path: &std::path::Path) -> Option<std::fs::File> {
    for i in (1..LOG_ROTATE_KEEP).rev() {
        let _ = std::fs::rename(
            path.with_extension(format!("log.{i}")),
            path.with_extension(format!("log.{}", i + 1)),
        );
    }
    let _ = std::fs::rename(path, path.with_extension("log.1"));
    std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .ok()
}

/// Print to the terminal and append the same line to the log file — without
/// doing either on the caller's thread. Enqueue is a `format` + `try_send`
/// (sub-microsecond); a full queue drops the line (counted, reported by the
/// writer) but the terminal mirror still fires from the recovered line, so a
/// stalled disk degrades visibility, never the caller.
pub fn log(msg: &str) {
    let line = Line { ms: now_ms(), msg: msg.to_string() };
    match LOG.get() {
        Some(tx) => {
            match tx.try_send(line) {
                Ok(()) => {}
                Err(std::sync::mpsc::TrySendError::Full(line))
                | Err(std::sync::mpsc::TrySendError::Disconnected(line)) => {
                    DROPPED.fetch_add(1, Ordering::Relaxed);
                    // The error carries the value back — the terminal mirror
                    // still fires for the dropped line.
                    eprintln!("{}", line.msg);
                }
            }
        }
        // init() not reached yet (or its thread failed to spawn) — the old
        // terminal-only behavior.
        None => eprintln!("{msg}"),
    }
}

/// `format!` convenience wrapper.
#[macro_export]
macro_rules! rlog {
    ($($arg:tt)*) => {
        $crate::logger::log(&format!($($arg)*))
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Unique per test (two tests once raced each other's temp dir —
    /// BUG-40's lesson applies to log files too).
    fn log_path(name: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("logger-test-{}-{name}", std::process::id()));
        let _ = std::fs::create_dir_all(&d);
        d.join("sakura-test.log")
    }

    /// Lines are written in order, stamped at enqueue time, one per line.
    #[test]
    fn writer_writes_lines_in_order() {
        let path = log_path("order");
        let _ = std::fs::remove_file(&path);
        let (tx, rx) = mpsc::sync_channel(8);
        tx.send(Line { ms: 111, msg: "first".into() }).unwrap();
        tx.send(Line { ms: 222, msg: "second".into() }).unwrap();
        drop(tx); // all senders gone → the writer drains and returns
        run_writer(rx, path.clone(), std::fs::OpenOptions::new().create(true).append(true).open(&path).ok());
        let text = std::fs::read_to_string(&path).unwrap();
        let a = text.find("111 first\n").expect("stamped first line missing");
        let b = text.find("222 second\n").expect("stamped second line missing");
        assert!(a < b, "line order must be preserved");
        let _ = std::fs::remove_file(&path);
    }

    /// Lines dropped on a full queue lead the next written line — disk stalls
    /// must be visible in the record, not silent.
    #[test]
    fn dropped_lines_are_reported_on_the_next_write() {
        let path = log_path("dropped");
        let _ = std::fs::remove_file(&path);
        let (tx, rx) = mpsc::sync_channel(8);
        let file = std::fs::OpenOptions::new().create(true).append(true).open(&path).ok();
        DROPPED.store(3, Ordering::Relaxed);
        tx.send(Line { ms: 9, msg: "through".into() }).unwrap();
        drop(tx);
        run_writer(rx, path.clone(), file);
        let text = std::fs::read_to_string(&path).unwrap();
        let r = text.find("3 log lines dropped").expect("drop report missing");
        let l = text.find("9 through\n").expect("through line missing");
        assert!(r < l, "the drop report must lead the next line");
        assert_eq!(DROPPED.load(Ordering::Relaxed), 0, "count must be reset");
        let _ = std::fs::remove_file(&path);
    }

    /// A full queue drops the line immediately (bounded memory, never a
    /// stalled caller) and the dropped line is recoverable from the error for
    /// the terminal mirror.
    #[test]
    fn full_queue_never_blocks_the_caller() {
        let (tx, rx) = mpsc::sync_channel::<Line>(1);
        tx.send(Line { ms: 1, msg: "only slot".into() }).unwrap();
        let before = DROPPED.load(Ordering::Relaxed);
        match tx.try_send(Line { ms: 2, msg: "overflow".into() }) {
            Ok(()) => panic!("queue should be full"),
            // The error carries the value back — the terminal mirror can
            // still print it.
            Err(std::sync::mpsc::TrySendError::Full(line))
            | Err(std::sync::mpsc::TrySendError::Disconnected(line)) => {
                assert_eq!(line.msg, "overflow");
            }
        }
        // The accounting itself is `log()`'s job; this test pins the
        // mechanism (non-blocking, recoverable), not the global state.
        assert_eq!(DROPPED.load(Ordering::Relaxed), before);
        drop(rx);
    }

    /// Rotation shifts every copy one slot down and reopens a fresh file;
    /// the oldest copy falls off (LOG_ROTATE_KEEP total).
    #[test]
    fn rotation_shifts_copies_and_reopens() {
        let dir = std::env::temp_dir().join(format!("logger-test-{}-rotate", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("sakura-test.log");
        std::fs::write(&path, "current").unwrap();
        std::fs::write(dir.join("sakura-test.log.1"), "one").unwrap();
        std::fs::write(dir.join("sakura-test.log.2"), "two").unwrap();
        let file = rotate(&path);
        assert!(file.is_some(), "rotate must reopen a fresh file");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "");
        assert_eq!(std::fs::read_to_string(dir.join("sakura-test.log.1")).unwrap(), "current");
        assert_eq!(std::fs::read_to_string(dir.join("sakura-test.log.2")).unwrap(), "one");
        assert_eq!(std::fs::read_to_string(dir.join("sakura-test.log.3")).unwrap(), "two");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
