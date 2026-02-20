//! Tab 1: Unified Overview — correlated events across all platforms.
//!
//! Columns: Project(16) | GitHub CI(12) | Railway(12) | Vercel(12) | Branch(12) | Age(8)

use ratatui::{
    layout::{Constraint, Rect},
    text::{Line, Span},
    widgets::{Block, Borders, Cell, Row, Table, TableState},
    Frame,
};

use crate::output::table::{format_age, format_duration};
use crate::tui::app::App;
use crate::tui::theme::Theme;
use crate::tui::widgets::status_spans;

/// Draw the Unified Overview table.
pub fn draw(frame: &mut Frame, area: Rect, app: &App, theme: &Theme) {
    // Header row (T4: bold + fg.subtle)
    let header_cells = ["Project", "GitHub CI", "Railway", "Vercel", "Branch", "Age"]
        .iter()
        .map(|h| Cell::from(*h).style(theme.t4()));
    let header = Row::new(header_cells).height(1);

    let rows: Vec<Row> = app
        .data
        .correlated
        .iter()
        .enumerate()
        .map(|(i, corr)| {
            let is_selected = i == app.selected_row;
            let row_style = if is_selected {
                theme.selected_row()
            } else {
                ratatui::style::Style::default()
            };

            // Project identifier: prefer Vercel/Railway title, fall back to SHA prefix
            let project_name = corr
                .vercel
                .as_ref()
                .and_then(|e| e.title.as_deref())
                .or_else(|| corr.railway.as_ref().and_then(|e| e.title.as_deref()))
                .or_else(|| corr.github.as_ref().and_then(|e| e.title.as_deref()))
                .or_else(|| {
                    corr.commit_sha
                        .as_deref()
                        .map(|s| if s.len() > 8 { &s[..8] } else { s })
                })
                .unwrap_or("-")
                .to_string();

            // Status badge cells for each platform
            let gh_cell = match corr.github.as_ref() {
                Some(e) => {
                    let (sym, label, style) = status_spans(&e.status, theme);
                    Cell::from(Line::from(vec![
                        Span::styled(sym, style),
                        Span::styled(label, style),
                    ]))
                }
                None => Cell::from(Span::styled("—", theme.t8())),
            };

            let rw_cell = match corr.railway.as_ref() {
                Some(e) => {
                    let (sym, label, style) = status_spans(&e.status, theme);
                    Cell::from(Line::from(vec![
                        Span::styled(sym, style),
                        Span::styled(label, style),
                    ]))
                }
                None => Cell::from(Span::styled("—", theme.t8())),
            };

            let vc_cell = match corr.vercel.as_ref() {
                Some(e) => {
                    let (sym, label, style) = status_spans(&e.status, theme);
                    Cell::from(Line::from(vec![
                        Span::styled(sym, style),
                        Span::styled(label, style),
                    ]))
                }
                None => Cell::from(Span::styled("—", theme.t8())),
            };

            let branch = corr
                .github
                .as_ref()
                .and_then(|e| e.branch.as_deref())
                .or_else(|| corr.vercel.as_ref().and_then(|e| e.branch.as_deref()))
                .unwrap_or("-");

            let age = format_age(corr.timestamp);

            // Stale indicator appended to age
            let age_display = if corr.is_stale {
                format!("{age} ●")
            } else {
                age
            };

            // Duration for reference (not shown as a column but kept for tooltip potential)
            let _duration = corr
                .github
                .as_ref()
                .and_then(|e| e.duration_secs)
                .map(format_duration)
                .unwrap_or_else(|| "-".into());

            Row::new(vec![
                Cell::from(Span::styled(project_name, theme.t5())),
                gh_cell,
                rw_cell,
                vc_cell,
                Cell::from(Span::styled(branch.to_string(), theme.t6())),
                Cell::from(Span::styled(age_display, theme.t8())),
            ])
            .style(row_style)
        })
        .collect();

    let widths = [
        Constraint::Length(16), // Project
        Constraint::Length(12), // GitHub CI
        Constraint::Length(12), // Railway
        Constraint::Length(12), // Vercel
        Constraint::Length(12), // Branch
        Constraint::Min(8),     // Age
    ];

    let table = Table::new(rows, widths)
        .header(header)
        .block(Block::default().borders(Borders::NONE))
        .row_highlight_style(theme.selected_row());

    let mut table_state = TableState::default();
    if !app.data.correlated.is_empty() {
        table_state.select(Some(app.selected_row));
    }

    frame.render_stateful_widget(table, area, &mut table_state);
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
                    is_from_cache: false,
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
                    is_from_cache: false,
                }),
                vercel: None,
                confidence: Confidence::High,
                timestamp: Utc::now(),
                is_stale: false,
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
                    is_from_cache: false,
                }),
                railway: None,
                vercel: None,
                confidence: Confidence::Unmatched,
                timestamp: Utc::now(),
                is_stale: false,
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

        let buf = terminal.backend().buffer().clone();
        let text = buffer_to_string(&buf);
        // Project column now shows title ("CI", "Deploy") rather than raw SHA
        assert!(
            text.contains("CI") || text.contains("abc123"),
            "Should contain project name or SHA"
        );
        assert!(text.contains("passed"), "Should contain success status");
        assert!(text.contains("failed"), "Should contain failed status");
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
