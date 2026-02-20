//! TUI widgets — reusable rendering components for each section of the dashboard.

pub mod footer;
pub mod header;
pub mod health;
pub mod logs;
pub mod platform;
pub mod settings;
pub mod unified;

use pulsos_core::domain::deployment::DeploymentStatus;
use ratatui::style::Style;

use crate::tui::theme::Theme;

/// Return `(symbol, label, Style)` for a deployment status badge (§4.1).
///
/// Used by `unified.rs` and `platform.rs` to build consistent status cells.
pub fn status_spans(status: &DeploymentStatus, theme: &Theme) -> (String, String, Style) {
    match status {
        DeploymentStatus::Success => ("✓ ".into(), "passed".into(), theme.success()),
        DeploymentStatus::Failed => ("✗ ".into(), "failed".into(), theme.failure()),
        DeploymentStatus::InProgress => ("◌ ".into(), "building".into(), theme.active()),
        DeploymentStatus::Queued => ("⏸ ".into(), "queued".into(), theme.neutral()),
        DeploymentStatus::Cancelled => ("— ".into(), "cancelled".into(), theme.neutral()),
        DeploymentStatus::Skipped => ("— ".into(), "skipped".into(), theme.neutral()),
        DeploymentStatus::ActionRequired => ("⚠ ".into(), "action".into(), theme.warning()),
        DeploymentStatus::Sleeping => ("● ".into(), "sleeping".into(), theme.neutral()),
        DeploymentStatus::Unknown(s) => ("? ".into(), s.clone(), theme.neutral()),
    }
}
