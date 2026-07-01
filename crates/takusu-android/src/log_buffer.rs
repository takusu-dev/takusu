//! In-process log ring buffer for the embedded Android server.
//!
//! A custom `tracing_subscriber` layer captures formatted log lines into a
//! bounded `VecDeque` so the mobile app can export them via UniFFI without
//! needing access to stderr/logcat.

use std::collections::VecDeque;
use std::sync::{Mutex, OnceLock};

use tracing_subscriber::fmt::MakeWriter;

/// Maximum number of log lines retained in the ring buffer.
const CAPACITY: usize = 2000;

struct LogBuffer {
    lines: VecDeque<String>,
}

impl LogBuffer {
    fn new() -> Self {
        Self {
            lines: VecDeque::with_capacity(CAPACITY),
        }
    }

    fn push(&mut self, line: String) {
        if self.lines.len() >= CAPACITY {
            self.lines.pop_front();
        }
        self.lines.push_back(line);
    }

    fn snapshot(&self) -> Vec<String> {
        self.lines.iter().cloned().collect()
    }

    fn clear(&mut self) {
        self.lines.clear();
    }
}

static BUFFER: OnceLock<Mutex<LogBuffer>> = OnceLock::new();

fn buffer() -> &'static Mutex<LogBuffer> {
    BUFFER.get_or_init(|| Mutex::new(LogBuffer::new()))
}

/// Writer that accumulates a single formatted log line and appends it to
/// the ring buffer when dropped (tracing_subscriber drops the writer after
/// each event is fully written).
pub struct BufferWriter {
    buf: String,
}

impl std::io::Write for BufferWriter {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.buf.push_str(&String::from_utf8_lossy(bytes));
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl Drop for BufferWriter {
    fn drop(&mut self) {
        // tracing_subscriber fmt layer writes a trailing newline; strip it
        // so each stored line is a single logical log entry.
        let line = self.buf.trim_end_matches('\n').to_string();
        if !line.is_empty()
            && let Ok(mut g) = buffer().lock()
        {
            g.push(line);
        }
    }
}

/// `MakeWriter` implementation that hands each log event a `BufferWriter`.
pub struct BufferMakeWriter;

impl<'a> MakeWriter<'a> for BufferMakeWriter {
    type Writer = BufferWriter;

    fn make_writer(&'a self) -> Self::Writer {
        BufferWriter { buf: String::new() }
    }
}

/// Install the ring-buffer subscriber. Called from `TakusuServer::start()`,
/// which may be invoked again after `stop()`, so this must be restart-safe.
/// `tracing_subscriber`'s global default can only be set once; subsequent
/// calls return `Err`, which we silently ignore — the existing subscriber
/// (with its ring-buffer layer) keeps capturing logs across restarts.
pub fn install() {
    use tracing_subscriber::Layer;
    use tracing_subscriber::layer::SubscriberExt;
    use tracing_subscriber::util::SubscriberInitExt;
    use tracing_subscriber::{EnvFilter, fmt};

    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("error,takusu_local=info,takusu_local_lib=info"));

    // The buffer layer formats each event into a single line and appends it.
    let buffer_layer = fmt::layer()
        .with_writer(BufferMakeWriter)
        .with_ansi(false)
        .with_target(true)
        .with_filter(env_filter.clone());

    let stderr_layer = fmt::layer()
        .with_writer(std::io::stderr)
        .with_ansi(false)
        .with_target(true)
        .with_filter(env_filter);

    // try_init() returns Err if a global subscriber is already set (e.g. on
    // server restart). That's fine — the first subscriber is still active and
    // its BufferMakeWriter still appends to the shared ring buffer.
    tracing_subscriber::registry()
        .with(buffer_layer)
        .with(stderr_layer)
        .try_init()
        .ok();
}

/// Snapshot of the captured log lines (oldest first).
pub fn get_logs() -> Vec<String> {
    match buffer().lock() {
        Ok(g) => g.snapshot(),
        Err(_) => vec![],
    }
}

/// Clear the captured log buffer.
pub fn clear_logs() {
    if let Ok(mut g) = buffer().lock() {
        g.clear();
    }
}

/// Push a log line from outside the `tracing` ecosystem (e.g. JS/Expo client).
/// The line is stored verbatim — no formatting is applied.
pub fn push_log(line: String) {
    if let Ok(mut g) = buffer().lock() {
        g.push(line);
    }
}
