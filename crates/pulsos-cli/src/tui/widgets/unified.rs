//! Tab 1: Unified Overview — correlated events across all platforms.
//!
//! Each row represents a commit SHA, showing status from GitHub, Railway, and Vercel
//! side by side, with a confidence indicator for the correlation.

use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::Span,
    widgets::{Block, Borders, Cell, Row, Table, TableState},
    Frame,
};

use crate::output::table::{format_age, format_duration, status_indicator};
use crate::tui::app::App;
use crate::tui::theme::Theme;
use pulsos_core::domain::deployment::DeploymentStatus;

/// Draw the Unified Overview table.
pub fn draw(frame: &mut Frame, area: Rect, app: &App, theme: &Theme) {
    let header_cells = [
        "Conf", "SHA", "GitHub", "Railway", "Vercel", "Branch", "Age", "Dur",
    ]
    .iter()
    .map(|h| Cell::from(*h).style(Style::default().add_modifier(Modifier::BOLD).fg(theme.fg)));
    let header = Row::new(header_cells).height(1);

    let rows: Vec<Row> = app
        .data
        .correlated
        .iter()
        .enumerate()
        .map(|(i, corr)| {
            let is_selected = i == app.selected_row;
            let row_style = if is_selected {
                theme.highlight
            } else {
                Style::default()
            };

            let confidence = corr.confidence.to_string();
            let sha = corr
                .commit_sha
                .as_deref()
                .map(|s| &s[..s.len().min(7)])
                .unwrap_or("-");

            let gh_status = corr
                .github
                .as_ref()
                .map(|e| status_indicator(&e.status))
                .unwrap_or_else(|| "-".into());
            let gh_color = corr
                .github
                .as_ref()
                .map(|e| status_color(&e.status, theme))
                .unwrap_or(theme.muted);

            let rw_status = corr
                .railway
                .as_ref()
                .map(|e| status_indicator(&e.status))
                .unwrap_or_else(|| "-".into());
            let rw_color = corr
                .railway
                .as_ref()
                .map(|e| status_color(&e.status, theme))
                .unwrap_or(theme.muted);

            let vc_status = corr
                .vercel
                .as_ref()
                .map(|e| status_indicator(&e.status))
                .unwrap_or_else(|| "-".into());
            let vc_color = corr
                .vercel
                .as_ref()
                .map(|e| status_color(&e.status, theme))
                .unwrap_or(theme.muted);

            let branch = corr
                .github
                .as_ref()
                .and_then(|e| e.branch.as_deref())
                .or_else(|| corr.vercel.as_ref().and_then(|e| e.branch.as_deref()))
                .unwrap_or("-");

            let age = format_age(corr.timestamp);
            let duration = corr
                .github
                .as_ref()
                .and_then(|e| e.duration_secs)
                .map(format_duration)
                .unwrap_or_else(|| "-".into());

            Row::new(vec![
                Cell::from(Span::raw(confidence)),
                Cell::from(Span::raw(sha.to_string())),
                Cell::from(Span::styled(gh_status, Style::default().fg(gh_color))),
                Cell::from(Span::styled(rw_status, Style::default().fg(rw_color))),
                Cell::from(Span::styled(vc_status, Style::default().fg(vc_color))),
                Cell::from(Span::raw(branch.to_string())),
                Cell::from(Span::raw(age)),
                Cell::from(Span::raw(duration)),
            ])
            .style(row_style)
        })
        .collect();

    let widths = [
        ratatui::layout::Constraint::Length(12), // Conf
        ratatui::layout::Constraint::Length(8),  // SHA
        ratatui::layout::Constraint::Length(8),  // GitHub
        ratatui::layout::Constraint::Length(8),  // Railway
        ratatui::layout::Constraint::Length(8),  // Vercel
        ratatui::layout::Constraint::Length(12), // Branch
        ratatui::layout::Constraint::Length(10), // Age
        ratatui::layout::Constraint::Min(6),     // Dur
    ];

    let table = Table::new(rows, widths)
        .header(header)
        .block(Block::default().borders(Borders::NONE))
        .row_highlight_style(theme.highlight);

    let mut table_state = TableState::default();
    if !app.data.correlated.is_empty() {
        table_state.select(Some(app.selected_row));
    }

    frame.render_stateful_widget(table, area, &mut table_state);
}

fn status_color(status: &DeploymentStatus, theme: &Theme) -> ratatui::style::Color {
    match status {
        DeploymentStatus::Success => theme.success,
        DeploymentStatus::Failed => theme.failure,
        DeploymentStatus::InProgress => theme.in_progress,
        DeploymentStatus::Queued => theme.queued,
        DeploymentStatus::Cancelled | DeploymentStatus::Skipped => theme.muted,
        DeploymentStatus::ActionRequired => theme.warning,
        DeploymentStatus::Sleeping => theme.muted,
        DeploymentStatus::Unknown(_) => theme.muted,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::app::{App, DataSnapshot};
    use chrono::Utc;
    use pulsos_core::config::types::TuiConfig;
    use pulsos_core::domain::deployment::{
        DeploymentEvent, DeploymentStatus, EventMetadata, Platform,
    };
    use pulsos_core::domain::project::{Confidence, CorrelatedEvent};
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    fn sample_correlated_events() -> Vec<CorrelatedEvent> {
        vec![
            CorrelatedEvent {
                commit_sha: Some("abc123def456".into()),
                github: Some(DeploymentEvent {
                    id: "run-1".into(),
                    platform: Platform::GitHub,
                    status: DeploymentStatus::Success,
                    commit_sha: Some("abc123def456".into()),
                    branch: Some("main".into()),
                    title: Some("CI".into()),
                    actor: Some("vivallo".into()),
                    created_at: Utc::now(),
                    updated_at: None,
                    duration_secs: Some(42),
                    url: None,
                    metadata: EventMetadata::default(),
                }),
                railway: Some(DeploymentEvent {
                    id: "rw-1".into(),
                    platform: Platform::Railway,
                    status: DeploymentStatus::Success,
                    commit_sha: None,
                    branch: None,
                    title: None,
                    actor: None,
                    created_at: Utc::now(),
                    updated_at: None,
                    duration_secs: None,
                    url: None,
                    metadata: EventMetadata::default(),
                }),
                vercel: None,
                confidence: Confidence::High,
                timestamp: Utc::now(),
            },
            CorrelatedEvent {
                commit_sha: Some("def456ghi789".into()),
                github: Some(DeploymentEvent {
                    id: "run-2".into(),
                    platform: Platform::GitHub,
                    status: DeploymentStatus::Failed,
                    commit_sha: Some("def456ghi789".into()),
                    branch: Some("feat".into()),
                    title: Some("Deploy".into()),
                    actor: Some("bot".into()),
                    created_at: Utc::now(),
                    updated_at: None,
                    duration_secs: Some(120),
                    url: None,
                    metadata: EventMetadata::default(),
                }),
                railway: None,
                vercel: None,
                confidence: Confidence::Unmatched,
                timestamp: Utc::now(),
            },
        ]
    }

    #[test]
    fn unified_tab_renders_without_panic() {
        let backend = TestBackend::new(100, 20);
        let mut terminal = Terminal::new(backend).unwrap();

        let mut data = DataSnapshot::default();
        data.correlated = sample_correlated_events();
        let app = App::new(data, TuiConfig::default());
        let theme = Theme::dark();

        terminal
            .draw(|frame| {
                draw(frame, frame.area(), &app, &theme);
            })
            .unwrap();

        // Verify buffer contains expected text
        let buf = terminal.backend().buffer().clone();
        let text = buffer_to_string(&buf);
        assert!(text.contains("abc123"), "Should contain SHA prefix");
        assert!(text.contains("OK"), "Should contain success status");
        assert!(text.contains("FAIL"), "Should contain failed status");
        assert!(text.contains("main"), "Should contain branch");
    }

    #[test]
    fn unified_tab_renders_empty_data() {
        let backend = TestBackend::new(100, 20);
        let mut terminal = Terminal::new(backend).unwrap();

        let data = DataSnapshot::default();
        let app = App::new(data, TuiConfig::default());
        let theme = Theme::dark();

        terminal
            .draw(|frame| {
                draw(frame, frame.area(), &app, &theme);
            })
            .unwrap();
        // Should not panic with empty data
    }

    fn buffer_to_string(buf: &ratatui::buffer::Buffer) -> String {
        let mut text = String::new();
        for y in buf.area.top()..buf.area.bottom() {
            for x in buf.area.left()..buf.area.right() {
                text.push_str(buf.cell((x, y)).map(|c| c.symbol()).unwrap_or(" "));
            }
        }
        text
    }
}
