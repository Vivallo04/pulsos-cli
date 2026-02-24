//! TUI widgets — reusable rendering components for each section of the dashboard.

pub mod footer;
pub mod header;
pub mod health;
pub mod logs;
pub mod platform;
pub mod settings;
pub mod unified;

use chrono::Utc;
use pulsos_core::domain::deployment::DeploymentStatus;
use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::Style,
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

use crate::tui::app::{App, InputMode};
use crate::tui::theme::Theme;

/// Split `area` into an optional search bar row + remaining content area.
///
/// When `app.input_mode == InputMode::Search`, returns `(Some(search_rect), content_rect)`.
/// Otherwise returns `(None, area)`.
pub fn split_search_bar(area: Rect, app: &App) -> (Option<Rect>, Rect) {
    if app.input_mode == InputMode::Search {
        let chunks = Layout::vertical([Constraint::Length(1), Constraint::Min(0)]).split(area);
        (Some(chunks[0]), chunks[1])
    } else {
        (None, area)
    }
}

/// Render the inline search bar: `/ {query}█`
pub fn draw_search_bar(frame: &mut Frame, area: Rect, app: &App, theme: &Theme) {
    let line = Line::from(vec![
        Span::styled("/ ", theme.neutral()),
        Span::styled(app.search_query.as_str(), theme.success()),
        Span::styled("█", theme.success()),
    ]);
    frame.render_widget(
        Paragraph::new(line).style(Style::new().bg(theme.bg_surface)),
        area,
    );
}

/// Return `(symbol, label, Style)` for a deployment status badge (§4.1).
///
/// Used by `unified.rs` and `platform.rs` to build consistent status cells.
pub fn status_spans(status: &DeploymentStatus, theme: &Theme) -> (String, String, Style) {
    match status {
        DeploymentStatus::Success => ("● ".into(), "passed".into(), theme.success()),
        DeploymentStatus::Failed => ("✕ ".into(), "failed".into(), theme.failure()),
        DeploymentStatus::InProgress => {
            const FRAMES: [&str; 4] = ["◐", "◓", "◑", "◒"];
            let idx = ((Utc::now().timestamp_millis() / 120) as usize) % FRAMES.len();
            (format!("{} ", FRAMES[idx]), "running".into(), theme.active())
        }
        DeploymentStatus::Queued => ("◌ ".into(), "queued".into(), theme.neutral()),
        DeploymentStatus::Cancelled => ("○ ".into(), "cancelled".into(), theme.neutral()),
        DeploymentStatus::Skipped => ("○ ".into(), "skipped".into(), theme.neutral()),
        DeploymentStatus::ActionRequired => ("⚠ ".into(), "action".into(), theme.warning()),
        DeploymentStatus::Sleeping => ("● ".into(), "sleeping".into(), theme.neutral()),
        DeploymentStatus::Unknown(s) => ("? ".into(), s.clone(), theme.neutral()),
    }
}
