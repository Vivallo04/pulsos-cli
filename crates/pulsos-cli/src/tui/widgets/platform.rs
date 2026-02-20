//! Tab 2: Platform Details — per-platform deployment event details.
//!
//! Shows all deployment events with platform-specific detail columns:
//! - GitHub: workflow_name + trigger_event
//! - Railway: service_name / environment_name
//! - Vercel: deploy_target + preview_url

use ratatui::{
    layout::Constraint,
    layout::Rect,
    text::{Line, Span},
    widgets::{Block, Borders, Cell, Row, Table, TableState},
    Frame,
};

use crate::output::table::{format_age, format_duration};
use crate::tui::app::App;
use crate::tui::theme::Theme;
use crate::tui::widgets::status_spans;
use pulsos_core::domain::deployment::{DeploymentEvent, Platform};

/// Draw the Platform Details table.
pub fn draw(frame: &mut Frame, area: Rect, app: &App, theme: &Theme) {
    let header_cells = [
        "Status", "Platform", "Title", "Detail", "Actor", "Age", "Dur",
    ]
    .iter()
    .map(|h| Cell::from(*h).style(theme.t4()));
    let header = Row::new(header_cells).height(1);

    // Apply search filter if active
    let filtered_events: Vec<&DeploymentEvent> = if app.search_query.is_empty() {
        app.data.events.iter().collect()
    } else {
        let q = app.search_query.to_ascii_lowercase();
        app.data
            .events
            .iter()
            .filter(|e| {
                e.title
                    .as_deref()
                    .unwrap_or("")
                    .to_ascii_lowercase()
                    .contains(&q)
                    || e.platform.to_string().to_ascii_lowercase().contains(&q)
                    || e.branch
                        .as_deref()
                        .unwrap_or("")
                        .to_ascii_lowercase()
                        .contains(&q)
                    || e.actor
                        .as_deref()
                        .unwrap_or("")
                        .to_ascii_lowercase()
                        .contains(&q)
            })
            .collect()
    };

    let rows: Vec<Row> = filtered_events
        .iter()
        .enumerate()
        .map(|(i, event)| {
            let is_selected = i == app.selected_row;
            let row_style = if is_selected {
                theme.selected_row()
            } else {
                ratatui::style::Style::default()
            };

            // Status badge
            let (sym, label, style) = status_spans(&event.status, theme);
            let status_cell = Cell::from(Line::from(vec![
                Span::styled(sym, style),
                Span::styled(label, style),
            ]));

            // Platform name — T6 (fg.default), no per-platform coloring
            let platform_text = event.platform.to_string();
            let platform_cell = Cell::from(Span::styled(platform_text, theme.t6()));

            let title = event
                .title
                .as_deref()
                .unwrap_or_else(|| &event.id[..event.id.len().min(12)]);

            let detail = platform_detail(event);
            let actor = event.actor.as_deref().unwrap_or("-");
            let age = format_age(event.created_at);
            let duration = event
                .duration_secs
                .map(format_duration)
                .unwrap_or_else(|| "-".into());

            Row::new(vec![
                status_cell,
                platform_cell,
                Cell::from(Span::styled(title.to_string(), theme.t6())),
                Cell::from(Span::styled(detail, theme.t7())),
                Cell::from(Span::styled(actor.to_string(), theme.t6())),
                Cell::from(Span::styled(age, theme.t8())),
                Cell::from(Span::styled(duration, theme.t8())),
            ])
            .style(row_style)
        })
        .collect();

    let widths = [
        Constraint::Length(12), // Status
        Constraint::Length(9),  // Platform
        Constraint::Length(16), // Title
        Constraint::Length(20), // Detail
        Constraint::Length(12), // Actor
        Constraint::Length(10), // Age
        Constraint::Min(6),     // Dur
    ];

    let table = Table::new(rows, widths)
        .header(header)
        .block(Block::default().borders(Borders::NONE))
        .row_highlight_style(theme.selected_row());

    let mut table_state = TableState::default();
    if !filtered_events.is_empty() {
        table_state.select(Some(
            app.selected_row
                .min(filtered_events.len().saturating_sub(1)),
        ));
    }

    frame.render_stateful_widget(table, area, &mut table_state);
}

/// Build a platform-specific detail string.
fn platform_detail(event: &DeploymentEvent) -> String {
    match event.platform {
        Platform::GitHub => {
            let workflow = event.metadata.workflow_name.as_deref().unwrap_or("");
            let trigger = event.metadata.trigger_event.as_deref().unwrap_or("");
            if workflow.is_empty() && trigger.is_empty() {
                "-".into()
            } else if trigger.is_empty() {
                workflow.into()
            } else {
                format!("{workflow} ({trigger})")
            }
        }
        Platform::Railway => {
            let service = event.metadata.service_name.as_deref().unwrap_or("");
            let env = event.metadata.environment_name.as_deref().unwrap_or("");
            if service.is_empty() && env.is_empty() {
                "-".into()
            } else if env.is_empty() {
                service.into()
            } else {
                format!("{service} / {env}")
            }
        }
        Platform::Vercel => {
            let target = event.metadata.deploy_target.as_deref().unwrap_or("");
            let preview = event.metadata.preview_url.as_deref().unwrap_or("");
            if target.is_empty() && preview.is_empty() {
                "-".into()
            } else if preview.is_empty() {
                target.into()
            } else {
                format!("{target} → {preview}")
            }
        }
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
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    fn sample_events() -> Vec<DeploymentEvent> {
        vec![
            DeploymentEvent {
                id: "run-1".into(),
                platform: Platform::GitHub,
                status: DeploymentStatus::Success,
                commit_sha: Some("abc123".into()),
                branch: Some("main".into()),
                title: Some("CI".into()),
                actor: Some("vivallo".into()),
                created_at: Utc::now(),
                updated_at: None,
                duration_secs: Some(42),
                url: None,
                metadata: EventMetadata {
                    workflow_name: Some("CI".into()),
                    trigger_event: Some("push".into()),
                    ..Default::default()
                },
                is_from_cache: false,
            },
            DeploymentEvent {
                id: "rw-1".into(),
                platform: Platform::Railway,
                status: DeploymentStatus::InProgress,
                commit_sha: None,
                branch: None,
                title: None,
                actor: None,
                created_at: Utc::now(),
                updated_at: None,
                duration_secs: None,
                url: None,
                metadata: EventMetadata {
                    service_name: Some("api".into()),
                    environment_name: Some("production".into()),
                    ..Default::default()
                },
                is_from_cache: false,
            },
            DeploymentEvent {
                id: "vc-1".into(),
                platform: Platform::Vercel,
                status: DeploymentStatus::Success,
                commit_sha: Some("abc123".into()),
                branch: Some("main".into()),
                title: Some("my-saas-web".into()),
                actor: Some("vivallo".into()),
                created_at: Utc::now(),
                updated_at: None,
                duration_secs: Some(30),
                url: None,
                metadata: EventMetadata {
                    deploy_target: Some("production".into()),
                    ..Default::default()
                },
                is_from_cache: false,
            },
        ]
    }

    #[test]
    fn platform_tab_renders_without_panic() {
        let backend = TestBackend::new(100, 20);
        let mut terminal = Terminal::new(backend).unwrap();

        let mut data = DataSnapshot::default();
        data.events = sample_events();
        let app = App::new(data, TuiConfig::default());
        let theme = Theme::dark();

        terminal
            .draw(|frame| {
                draw(frame, frame.area(), &app, &theme);
            })
            .unwrap();

        let buf = terminal.backend().buffer().clone();
        let text = buffer_to_string(&buf);
        assert!(text.contains("GitHub"), "Should show GitHub platform");
        assert!(text.contains("Railway"), "Should show Railway platform");
        assert!(text.contains("Vercel"), "Should show Vercel platform");
        assert!(text.contains("vivallo"), "Should show actor");
    }

    #[test]
    fn platform_detail_github() {
        let event = DeploymentEvent {
            id: "x".into(),
            platform: Platform::GitHub,
            status: DeploymentStatus::Success,
            commit_sha: None,
            branch: None,
            title: None,
            actor: None,
            created_at: Utc::now(),
            updated_at: None,
            duration_secs: None,
            url: None,
            metadata: EventMetadata {
                workflow_name: Some("CI".into()),
                trigger_event: Some("push".into()),
                ..Default::default()
            },
            is_from_cache: false,
        };
        assert_eq!(platform_detail(&event), "CI (push)");
    }

    #[test]
    fn platform_detail_railway() {
        let event = DeploymentEvent {
            id: "x".into(),
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
            metadata: EventMetadata {
                service_name: Some("api".into()),
                environment_name: Some("prod".into()),
                ..Default::default()
            },
            is_from_cache: false,
        };
        assert_eq!(platform_detail(&event), "api / prod");
    }

    #[test]
    fn platform_detail_vercel() {
        let event = DeploymentEvent {
            id: "x".into(),
            platform: Platform::Vercel,
            status: DeploymentStatus::Success,
            commit_sha: None,
            branch: None,
            title: None,
            actor: None,
            created_at: Utc::now(),
            updated_at: None,
            duration_secs: None,
            url: None,
            metadata: EventMetadata {
                deploy_target: Some("production".into()),
                preview_url: Some("abc.vercel.app".into()),
                ..Default::default()
            },
            is_from_cache: false,
        };
        assert_eq!(platform_detail(&event), "production → abc.vercel.app");
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
