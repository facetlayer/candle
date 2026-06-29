//! Capturable global output sink.
//!
//! Command handlers emit their human-facing lines through [`out`] and [`err`]
//! instead of `println!`/`eprintln!` directly. By default these pass through to
//! the real stdout/stderr. When a [`capture`] scope is active (on the current
//! thread), the lines are buffered instead so a caller — notably the MCP server
//! (milestone M8) — can collect a handler's output rather than letting it write
//! to the process's real streams.
//!
//! Capture state is a thread-local: handlers run synchronously on the same
//! thread as the `capture` call, so there is no need for cross-thread sharing.

use std::cell::RefCell;

#[derive(Default)]
struct Buffer {
    stdout: Vec<String>,
    stderr: Vec<String>,
    /// stdout + stderr in emission order, used to build a combined transcript.
    combined: Vec<String>,
}

thread_local! {
    static CAPTURE: RefCell<Option<Buffer>> = const { RefCell::new(None) };
}

/// Output collected during a [`capture`] scope.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CapturedOutput {
    /// Lines emitted via [`out`], in order.
    pub stdout: Vec<String>,
    /// Lines emitted via [`err`], in order.
    pub stderr: Vec<String>,
    /// Both streams interleaved in emission order.
    combined: Vec<String>,
}

impl CapturedOutput {
    /// A single combined transcript (stdout + stderr in emission order, one line
    /// per entry). Useful for surfacing a handler's output to MCP clients.
    pub fn transcript(&self) -> String {
        self.combined.join("\n")
    }
}

/// Emit a line to stdout, or buffer it when a [`capture`] scope is active.
pub fn out(line: &str) {
    CAPTURE.with(|cell| {
        let mut slot = cell.borrow_mut();
        match slot.as_mut() {
            Some(buf) => {
                buf.stdout.push(line.to_string());
                buf.combined.push(line.to_string());
            }
            None => println!("{line}"),
        }
    });
}

/// Emit a line to stderr, or buffer it when a [`capture`] scope is active.
pub fn err(line: &str) {
    CAPTURE.with(|cell| {
        let mut slot = cell.borrow_mut();
        match slot.as_mut() {
            Some(buf) => {
                buf.stderr.push(line.to_string());
                buf.combined.push(line.to_string());
            }
            None => eprintln!("{line}"),
        }
    });
}

/// Run `f` with output capture active on the current thread, returning its value
/// alongside everything it emitted via [`out`]/[`err`].
pub fn capture<T>(f: impl FnOnce() -> T) -> (T, CapturedOutput) {
    CAPTURE.with(|cell| *cell.borrow_mut() = Some(Buffer::default()));

    // Even if `f` panics we want to clear the capture slot, so restore on unwind.
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(f));

    let buffer = CAPTURE
        .with(|cell| cell.borrow_mut().take())
        .unwrap_or_default();

    let captured = CapturedOutput {
        stdout: buffer.stdout,
        stderr: buffer.stderr,
        combined: buffer.combined,
    };

    match result {
        Ok(value) => (value, captured),
        Err(payload) => std::panic::resume_unwind(payload),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capture_collects_out_and_err() {
        let (ret, captured) = capture(|| {
            out("hello");
            err("oops");
            out("world");
            42
        });

        assert_eq!(ret, 42);
        assert_eq!(captured.stdout, vec!["hello".to_string(), "world".to_string()]);
        assert_eq!(captured.stderr, vec!["oops".to_string()]);
        // Combined transcript preserves emission order across both streams.
        assert_eq!(captured.transcript(), "hello\noops\nworld");
    }

    #[test]
    fn capture_is_scoped() {
        // After a capture scope ends, the slot is cleared. A fresh capture starts
        // empty (passthrough is restored outside the scope).
        let (_, first) = capture(|| out("a"));
        assert_eq!(first.stdout, vec!["a".to_string()]);

        let (_, second) = capture(|| {});
        assert!(second.stdout.is_empty());
        assert!(second.stderr.is_empty());
        assert_eq!(second.transcript(), "");
    }
}
