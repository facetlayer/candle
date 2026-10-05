//! Thread-local output capture for synchronous CLI and MCP handlers.
//! Outside a [`capture`] scope, output goes to stdout/stderr.

use std::cell::RefCell;

#[derive(Default)]
struct Buffer {
    stdout: Vec<String>,
    stderr: Vec<String>,
    /// `(is_stderr, line)` entries in emission order.
    combined: Vec<(bool, String)>,
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
    /// Both streams interleaved in emission order. Each entry is `(is_stderr, line)`.
    combined: Vec<(bool, String)>,
}

impl CapturedOutput {
    /// Combine both streams in emission order, one line per entry.
    pub fn transcript(&self) -> String {
        self.combined
            .iter()
            .map(|(_, line)| line.as_str())
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Format an MCP transcript, prefixing stderr lines with `[stderr] `.
    pub fn mcp_log_lines(&self) -> Vec<String> {
        self.combined
            .iter()
            .map(|(is_stderr, line)| {
                if *is_stderr {
                    format!("[stderr] {line}")
                } else {
                    line.clone()
                }
            })
            .collect()
    }
}

/// Emit a line to stdout, or buffer it when a [`capture`] scope is active.
pub fn out(line: &str) {
    CAPTURE.with(|cell| {
        let mut slot = cell.borrow_mut();
        match slot.as_mut() {
            Some(buf) => {
                buf.stdout.push(line.to_string());
                buf.combined.push((false, line.to_string()));
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
                buf.combined.push((true, line.to_string()));
            }
            None => eprintln!("{line}"),
        }
    });
}

/// Emit or capture an error formatted by [`crate::errors::error_line`].
pub fn error(message: &str) {
    err(&crate::errors::error_line(message));
}

/// Run `f` with output capture active on the current thread, returning its value
/// alongside everything it emitted via [`out`]/[`err`].
pub fn capture<T>(f: impl FnOnce() -> T) -> (T, CapturedOutput) {
    CAPTURE.with(|cell| *cell.borrow_mut() = Some(Buffer::default()));

    // Restore capture state even if the handler panics.
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
        assert_eq!(
            captured.stdout,
            vec!["hello".to_string(), "world".to_string()]
        );
        assert_eq!(captured.stderr, vec!["oops".to_string()]);
        assert_eq!(captured.transcript(), "hello\noops\nworld");
        assert_eq!(
            captured.mcp_log_lines(),
            vec![
                "hello".to_string(),
                "[stderr] oops".to_string(),
                "world".to_string()
            ]
        );
    }

    #[test]
    fn capture_is_scoped() {
        let (_, first) = capture(|| out("a"));
        assert_eq!(first.stdout, vec!["a".to_string()]);

        let (_, second) = capture(|| {});
        assert!(second.stdout.is_empty());
        assert!(second.stderr.is_empty());
        assert_eq!(second.transcript(), "");
    }
}
