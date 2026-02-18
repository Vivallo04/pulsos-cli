//! Diagnostic output formatting for `pulsos doctor`.
//!
//! Provides reusable formatting primitives: status indicators (✓/⚠/✗/─),
//! aligned check results, section headers, and summary lines.

use std::time::Duration;

/// Status indicator for a diagnostic check.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckStatus {
    Ok,
    Warning,
    Error,
    Skipped,
}

/// A single diagnostic result line.
#[derive(Debug)]
pub struct CheckResult {
    pub status: CheckStatus,
    pub label: String,
    pub value: String,
    /// Optional detail line printed indented below the main line.
    pub detail: Option<String>,
}

impl CheckResult {
    pub fn ok(label: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            status: CheckStatus::Ok,
            label: label.into(),
            value: value.into(),
            detail: None,
        }
    }

    pub fn warning(label: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            status: CheckStatus::Warning,
            label: label.into(),
            value: value.into(),
            detail: None,
        }
    }

    pub fn error(label: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            status: CheckStatus::Error,
            label: label.into(),
            value: value.into(),
            detail: None,
        }
    }

    pub fn skipped(label: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            status: CheckStatus::Skipped,
            label: label.into(),
            value: value.into(),
            detail: None,
        }
    }

    pub fn with_detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }
}

/// Returns the Unicode status indicator.
pub fn status_icon(status: CheckStatus) -> &'static str {
    match status {
        CheckStatus::Ok => "✓",
        CheckStatus::Warning => "⚠",
        CheckStatus::Error => "✗",
        CheckStatus::Skipped => "─",
    }
}

/// Print a section header.
pub fn print_section(title: &str) {
    println!("  {title}");
}

/// Print a single check result with aligned columns.
///
/// Format: `    Label:         value                    ✓`
pub fn print_check(result: &CheckResult) {
    println!(
        "    {:<16} {:<40} {}",
        format!("{}:", result.label),
        result.value,
        status_icon(result.status),
    );
    if let Some(ref detail) = result.detail {
        println!("    {:<16} {}", "", detail);
    }
}

/// Print the final summary with actionable suggestions.
pub fn print_summary(warnings: usize, errors: usize, suggestions: &[String]) {
    if warnings == 0 && errors == 0 {
        println!("  Result: all checks passed ✓");
    } else {
        let parts: Vec<String> = [
            (errors > 0).then(|| format!("{errors} error{}", if errors == 1 { "" } else { "s" })),
            (warnings > 0)
                .then(|| format!("{warnings} warning{}", if warnings == 1 { "" } else { "s" })),
        ]
        .into_iter()
        .flatten()
        .collect();

        println!("  Result: {}", parts.join(", "));
    }

    for suggestion in suggestions {
        println!("    → {suggestion}");
    }
}

/// Format a byte count as a human-readable string.
pub fn format_bytes(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = 1024 * 1024;
    const GB: u64 = 1024 * 1024 * 1024;

    if bytes >= GB {
        format!("{:.1} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1} KB", bytes as f64 / KB as f64)
    } else {
        format!("{bytes} B")
    }
}

/// Format a duration as a latency string (e.g., "45ms", "1.2s").
pub fn format_latency(duration: Duration) -> String {
    let ms = duration.as_millis();
    if ms < 1000 {
        format!("{ms}ms")
    } else {
        format!("{:.1}s", duration.as_secs_f64())
    }
}

/// Count warnings and errors from a check result.
pub fn count_issues(result: &CheckResult, warnings: &mut usize, errors: &mut usize) {
    match result.status {
        CheckStatus::Warning => *warnings += 1,
        CheckStatus::Error => *errors += 1,
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_icon_variants() {
        assert_eq!(status_icon(CheckStatus::Ok), "✓");
        assert_eq!(status_icon(CheckStatus::Warning), "⚠");
        assert_eq!(status_icon(CheckStatus::Error), "✗");
        assert_eq!(status_icon(CheckStatus::Skipped), "─");
    }

    #[test]
    fn format_bytes_b() {
        assert_eq!(format_bytes(0), "0 B");
        assert_eq!(format_bytes(512), "512 B");
        assert_eq!(format_bytes(1023), "1023 B");
    }

    #[test]
    fn format_bytes_kb() {
        assert_eq!(format_bytes(1024), "1.0 KB");
        assert_eq!(format_bytes(1536), "1.5 KB");
        assert_eq!(format_bytes(10 * 1024), "10.0 KB");
    }

    #[test]
    fn format_bytes_mb() {
        assert_eq!(format_bytes(1024 * 1024), "1.0 MB");
        assert_eq!(format_bytes(2_400_000), "2.3 MB");
    }

    #[test]
    fn format_bytes_gb() {
        assert_eq!(format_bytes(1024 * 1024 * 1024), "1.0 GB");
        assert_eq!(format_bytes(2 * 1024 * 1024 * 1024), "2.0 GB");
    }

    #[test]
    fn format_latency_ms() {
        assert_eq!(format_latency(Duration::from_millis(45)), "45ms");
        assert_eq!(format_latency(Duration::from_millis(0)), "0ms");
        assert_eq!(format_latency(Duration::from_millis(999)), "999ms");
    }

    #[test]
    fn format_latency_seconds() {
        assert_eq!(format_latency(Duration::from_millis(1000)), "1.0s");
        assert_eq!(format_latency(Duration::from_millis(1500)), "1.5s");
        assert_eq!(format_latency(Duration::from_secs(3)), "3.0s");
    }

    #[test]
    fn check_result_constructors() {
        let ok = CheckResult::ok("GitHub", "authenticated");
        assert_eq!(ok.status, CheckStatus::Ok);
        assert_eq!(ok.label, "GitHub");
        assert_eq!(ok.value, "authenticated");
        assert!(ok.detail.is_none());

        let warn = CheckResult::warning("Vercel", "expires soon").with_detail("7 days remaining");
        assert_eq!(warn.status, CheckStatus::Warning);
        assert_eq!(warn.detail, Some("7 days remaining".into()));

        let err = CheckResult::error("Railway", "auth failed");
        assert_eq!(err.status, CheckStatus::Error);

        let skip = CheckResult::skipped("gh CLI", "not installed");
        assert_eq!(skip.status, CheckStatus::Skipped);
    }

    #[test]
    fn count_issues_tracks_correctly() {
        let mut warnings = 0;
        let mut errors = 0;

        count_issues(&CheckResult::ok("a", "b"), &mut warnings, &mut errors);
        assert_eq!((warnings, errors), (0, 0));

        count_issues(&CheckResult::warning("a", "b"), &mut warnings, &mut errors);
        assert_eq!((warnings, errors), (1, 0));

        count_issues(&CheckResult::error("a", "b"), &mut warnings, &mut errors);
        assert_eq!((warnings, errors), (1, 1));

        count_issues(&CheckResult::skipped("a", "b"), &mut warnings, &mut errors);
        assert_eq!((warnings, errors), (1, 1));
    }
}
