//! In-memory log ring buffer and dual-writer for TUI log capture.
//!
//! When the TUI is active, tracing logs are captured into a `LogRingBuffer`
//! and suppressed from stderr. When the TUI is inactive (or for non-TUI
//! commands), logs pass through to stderr as normal.

use std::collections::VecDeque;
use std::io;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use chrono::{DateTime, Utc};
use tracing::Level;
use tracing_subscriber::fmt::MakeWriter;

/// Maximum number of log entries retained in the ring buffer.
const RING_CAPACITY: usize = 500;

/// A single captured log entry.
#[derive(Debug, Clone)]
pub struct LogEntry {
    pub timestamp: DateTime<Utc>,
    pub level: Level,
    pub target: String,
    pub message: String,
}

/// Thread-safe ring buffer holding recent log entries.
#[derive(Debug, Clone)]
pub struct LogRingBuffer {
    inner: Arc<Mutex<VecDeque<LogEntry>>>,
}

impl LogRingBuffer {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(VecDeque::with_capacity(RING_CAPACITY))),
        }
    }

    /// Push a new log entry, evicting the oldest if at capacity.
    pub fn push(&self, entry: LogEntry) {
        let mut buf = self.inner.lock().unwrap();
        if buf.len() >= RING_CAPACITY {
            buf.pop_front();
        }
        buf.push_back(entry);
    }

    /// Return a snapshot of all entries (oldest first).
    pub fn snapshot(&self) -> Vec<LogEntry> {
        self.inner.lock().unwrap().iter().cloned().collect()
    }

    /// Return the most recent entry, if any.
    pub fn latest(&self) -> Option<LogEntry> {
        self.inner.lock().unwrap().back().cloned()
    }

    /// Number of entries currently stored.
    pub fn len(&self) -> usize {
        self.inner.lock().unwrap().len()
    }
}

/// Atomic flag indicating whether the TUI is currently active.
///
/// When active, the `DualWriter` suppresses stderr output.
#[derive(Debug, Clone)]
pub struct TuiActiveFlag {
    flag: Arc<AtomicBool>,
}

impl TuiActiveFlag {
    pub fn new() -> Self {
        Self {
            flag: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn set_active(&self, active: bool) {
        self.flag.store(active, Ordering::SeqCst);
    }

    pub fn is_active(&self) -> bool {
        self.flag.load(Ordering::SeqCst)
    }
}

/// A `MakeWriter` that produces `DualWriter` instances.
///
/// Each writer buffers formatted log output until a newline, then:
/// - Always pushes a `LogEntry` to the ring buffer
/// - Conditionally writes to stderr (suppressed when TUI is active)
#[derive(Clone)]
pub struct DualWriterFactory {
    buffer: LogRingBuffer,
    tui_active: TuiActiveFlag,
}

impl DualWriterFactory {
    pub fn new(buffer: LogRingBuffer, tui_active: TuiActiveFlag) -> Self {
        Self { buffer, tui_active }
    }
}

impl<'a> MakeWriter<'a> for DualWriterFactory {
    type Writer = DualWriter;

    fn make_writer(&'a self) -> Self::Writer {
        DualWriter {
            buffer: self.buffer.clone(),
            tui_active: self.tui_active.clone(),
            line_buf: Vec::new(),
            level_hint: None,
            target_hint: None,
        }
    }

    fn make_writer_for(&'a self, meta: &tracing::Metadata<'_>) -> Self::Writer {
        DualWriter {
            buffer: self.buffer.clone(),
            tui_active: self.tui_active.clone(),
            line_buf: Vec::new(),
            level_hint: Some(*meta.level()),
            target_hint: Some(meta.target().to_string()),
        }
    }
}

/// A single-use writer that captures one log line.
pub struct DualWriter {
    buffer: LogRingBuffer,
    tui_active: TuiActiveFlag,
    line_buf: Vec<u8>,
    level_hint: Option<Level>,
    target_hint: Option<String>,
}

impl DualWriter {
    /// Parse the tracing level from formatted output as a fallback.
    fn parse_level_from_text(text: &str) -> Level {
        let upper = text.to_ascii_uppercase();
        if upper.contains("ERROR") {
            Level::ERROR
        } else if upper.contains("WARN") {
            Level::WARN
        } else if upper.contains("DEBUG") {
            Level::DEBUG
        } else if upper.contains("TRACE") {
            Level::TRACE
        } else {
            Level::INFO
        }
    }

    /// Strip the ANSI-colored level prefix and timestamp that tracing_subscriber adds.
    fn strip_prefix(text: &str) -> String {
        // tracing_subscriber fmt produces lines like:
        //   "  2024-01-01T00:00:00Z  WARN message\n"
        // or with colors:
        //   "  \x1b[33m WARN\x1b[0m message\n"
        // We want just the message part.
        let trimmed = text.trim();

        // Try to find the level keyword and take everything after it
        for keyword in &["ERROR", "WARN", "INFO", "DEBUG", "TRACE"] {
            if let Some(pos) = trimmed.find(keyword) {
                let after = &trimmed[pos + keyword.len()..];
                let msg = after.trim_start_matches(':').trim();
                if !msg.is_empty() {
                    return msg.to_string();
                }
            }
        }

        trimmed.to_string()
    }
}

impl io::Write for DualWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.line_buf.extend_from_slice(buf);

        // Process complete lines
        while let Some(pos) = self.line_buf.iter().position(|&b| b == b'\n') {
            let line_bytes: Vec<u8> = self.line_buf.drain(..=pos).collect();
            let text = String::from_utf8_lossy(&line_bytes);

            let level = self
                .level_hint
                .unwrap_or_else(|| Self::parse_level_from_text(&text));
            let message = Self::strip_prefix(&text);

            if !message.is_empty() {
                self.buffer.push(LogEntry {
                    timestamp: Utc::now(),
                    level,
                    target: self.target_hint.clone().unwrap_or_default(),
                    message,
                });
            }

            // Write to stderr if TUI is not active
            if !self.tui_active.is_active() {
                let _ = io::stderr().write_all(&line_bytes);
            }
        }

        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        // Flush any remaining partial line
        if !self.line_buf.is_empty() {
            let text = String::from_utf8_lossy(&self.line_buf);
            let level = self
                .level_hint
                .unwrap_or_else(|| Self::parse_level_from_text(&text));
            let message = Self::strip_prefix(&text);

            if !message.is_empty() {
                self.buffer.push(LogEntry {
                    timestamp: Utc::now(),
                    level,
                    target: self.target_hint.clone().unwrap_or_default(),
                    message,
                });
            }

            if !self.tui_active.is_active() {
                let _ = io::stderr().write_all(&self.line_buf);
                let _ = io::stderr().flush();
            }

            self.line_buf.clear();
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn ring_buffer_push_and_snapshot() {
        let buf = LogRingBuffer::new();
        assert_eq!(buf.len(), 0);
        assert!(buf.latest().is_none());

        buf.push(LogEntry {
            timestamp: Utc::now(),
            level: Level::INFO,
            target: String::new(),
            message: "hello".into(),
        });
        assert_eq!(buf.len(), 1);
        assert_eq!(buf.latest().unwrap().message, "hello");

        buf.push(LogEntry {
            timestamp: Utc::now(),
            level: Level::WARN,
            target: String::new(),
            message: "world".into(),
        });
        assert_eq!(buf.len(), 2);
        assert_eq!(buf.latest().unwrap().message, "world");

        let snap = buf.snapshot();
        assert_eq!(snap.len(), 2);
        assert_eq!(snap[0].message, "hello");
        assert_eq!(snap[1].message, "world");
    }

    #[test]
    fn ring_buffer_capacity_eviction() {
        let buf = LogRingBuffer::new();
        for i in 0..600 {
            buf.push(LogEntry {
                timestamp: Utc::now(),
                level: Level::INFO,
                target: String::new(),
                message: format!("msg-{i}"),
            });
        }
        assert_eq!(buf.len(), RING_CAPACITY);
        let snap = buf.snapshot();
        // Oldest entries (0..99) should have been evicted
        assert_eq!(snap[0].message, "msg-100");
        assert_eq!(snap.last().unwrap().message, "msg-599");
    }

    #[test]
    fn tui_active_flag_toggle() {
        let flag = TuiActiveFlag::new();
        assert!(!flag.is_active());
        flag.set_active(true);
        assert!(flag.is_active());
        flag.set_active(false);
        assert!(!flag.is_active());
    }

    #[test]
    fn dual_writer_captures_to_buffer() {
        let buffer = LogRingBuffer::new();
        let tui_active = TuiActiveFlag::new();
        tui_active.set_active(true); // suppress stderr

        let factory = DualWriterFactory::new(buffer.clone(), tui_active);
        let mut writer = factory.make_writer();
        writer.write_all(b"  WARN some warning message\n").unwrap();
        writer.flush().unwrap();

        assert_eq!(buffer.len(), 1);
        let entry = buffer.latest().unwrap();
        assert_eq!(entry.level, Level::WARN);
        assert_eq!(entry.message, "some warning message");
    }

    #[test]
    fn dual_writer_with_level_hint() {
        let buffer = LogRingBuffer::new();
        let tui_active = TuiActiveFlag::new();
        tui_active.set_active(true);

        let factory = DualWriterFactory::new(buffer.clone(), tui_active);

        // Simulate make_writer_for with a known level
        let mut writer = DualWriter {
            buffer: buffer.clone(),
            tui_active: TuiActiveFlag::new(),
            line_buf: Vec::new(),
            level_hint: Some(Level::ERROR),
            target_hint: None,
        };
        writer
            .write_all(b"  ERROR connection refused\n")
            .unwrap();
        writer.flush().unwrap();

        let entry = buffer.latest().unwrap();
        assert_eq!(entry.level, Level::ERROR);

        // Make sure factory is used (suppress unused warning)
        let _ = factory.make_writer();
    }

    #[test]
    fn parse_level_from_text() {
        assert_eq!(DualWriter::parse_level_from_text("  WARN foo"), Level::WARN);
        assert_eq!(
            DualWriter::parse_level_from_text("  ERROR bar"),
            Level::ERROR
        );
        assert_eq!(
            DualWriter::parse_level_from_text("  DEBUG baz"),
            Level::DEBUG
        );
        assert_eq!(
            DualWriter::parse_level_from_text("  TRACE qux"),
            Level::TRACE
        );
        assert_eq!(
            DualWriter::parse_level_from_text("something else"),
            Level::INFO
        );
    }

    #[test]
    fn strip_prefix_extracts_message() {
        assert_eq!(
            DualWriter::strip_prefix("  WARN retry attempt 1"),
            "retry attempt 1"
        );
        assert_eq!(
            DualWriter::strip_prefix("  ERROR connection failed"),
            "connection failed"
        );
        assert_eq!(DualWriter::strip_prefix("  just text"), "just text");
    }
}
