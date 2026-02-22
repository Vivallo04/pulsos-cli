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
use crate::tui::widgets::{draw_search_bar, split_search_bar, status_spans};
use pulsos_core::domain::deployment::{DeploymentEvent, JobSummary, Platform};

/// Draw the Platform Details table.
pub fn draw(frame: &mut Frame, area: Rect, app: &App, theme: &Theme) {
    let (search_area, area) = split_search_bar(area, app);
    if let Some(sa) = search_area {
        draw_search_bar(frame, sa, app, theme);
    }

    let header_cells = [
        "Status", "Platform", "Title", "SHA", "Detail", "Actor", "Age", "Dur",
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
        .map(|event| {
            // Status badge
            let (sym, label, style) = status_spans(&event.status, theme);
            let status_cell = Cell::from(Line::from(vec![
                Span::styled(sym, style),
                Span::styled(label, style),
            ]));

            // Platform name — accent-colored per platform
            let platform_text = event.platform.to_string();
            let platform_style = match event.platform {
                Platform::GitHub => theme.gh_accent(),
                Platform::Railway => theme.rw_accent(),
                Platform::Vercel => theme.vc_accent(),
            };
            let platform_cell = Cell::from(Span::styled(platform_text, platform_style));

            let title = event
                .title
                .as_deref()
                .unwrap_or_else(|| &event.id[..event.id.len().min(12)]);

            // SHA cell: first 7 chars
            let sha = event
                .commit_sha
                .as_deref()
                .map(|s| if s.len() > 7 { &s[..7] } else { s })
                .unwrap_or("-");

            let detail_line = platform_detail(event, theme);
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
                Cell::from(Span::styled(sha.to_string(), theme.active())),
                Cell::from(detail_line),
                Cell::from(Span::styled(actor.to_string(), theme.t6())),
                Cell::from(Span::styled(age, theme.t8())),
                Cell::from(Span::styled(duration, theme.t8())),
            ])
        })
        .collect();

    let widths = [
        Constraint::Length(12), // Status
        Constraint::Length(8),  // Platform
        Constraint::Length(22), // Title
        Constraint::Length(9),  // SHA
        Constraint::Min(36),    // Detail
        Constraint::Length(12), // Actor
        Constraint::Length(7),  // Age
        Constraint::Length(8),  // Dur
    ];

    let table = Table::new(rows, widths)
        .header(header)
        .block(Block::default().borders(Borders::NONE))
        .row_highlight_style(theme.selected_row())
        .highlight_symbol("▶ ");

    let mut table_state = TableState::default();
    if !filtered_events.is_empty() {
        table_state.select(Some(
            app.selected_row
                .min(filtered_events.len().saturating_sub(1)),
        ));
    }

    frame.render_stateful_widget(table, area, &mut table_state);
}

/// Build a platform-specific detail line (supports styled pipeline stages for GitHub).
fn platform_detail(event: &DeploymentEvent, theme: &Theme) -> Line<'static> {
    match event.platform {
        Platform::GitHub if !event.metadata.jobs.is_empty() => {
            render_pipeline_stages(&event.metadata.jobs, theme)
        }
        Platform::GitHub => {
            let workflow = event.metadata.workflow_name.as_deref().unwrap_or("");
            let trigger = event.metadata.trigger_event.as_deref().unwrap_or("");
            let text = if workflow.is_empty() && trigger.is_empty() {
                "-".to_string()
            } else if trigger.is_empty() {
                workflow.to_string()
            } else {
                format!("{workflow} ({trigger})")
            };
            Line::from(Span::styled(text, theme.t7()))
        }
        Platform::Railway => {
            let service = event.metadata.service_name.as_deref().unwrap_or("");
            let env = event.metadata.environment_name.as_deref().unwrap_or("");
            let text = if service.is_empty() && env.is_empty() {
                "-".to_string()
            } else if env.is_empty() {
                service.to_string()
            } else {
                format!("{service} / {env}")
            };
            Line::from(Span::styled(text, theme.t7()))
        }
        Platform::Vercel => {
            let target = event.metadata.deploy_target.as_deref().unwrap_or("");
            let preview = event.metadata.preview_url.as_deref().unwrap_or("");
            let text = if target.is_empty() && preview.is_empty() {
                "-".to_string()
            } else if preview.is_empty() {
                target.to_string()
            } else {
                format!("{target} → {preview}")
            };
            Line::from(Span::styled(text, theme.t7()))
        }
    }
}

/// Render pipeline stages as `● Build › ● Test › ✕ Deploy`.
fn render_pipeline_stages(jobs: &[JobSummary], theme: &Theme) -> Line<'static> {
    let mut spans: Vec<Span<'static>> = Vec::new();
    for (i, job) in jobs.iter().enumerate() {
        if i > 0 {
            spans.push(Span::styled(" › ".to_string(), theme.t8()));
        }
        let (icon, _label, style) = status_spans(&job.status, theme);
        let short_name: String = if job.name.len() > 8 {
            format!("{}…", &job.name[..7])
        } else {
            job.name.clone()
        };
        spans.push(Span::styled(format!("{icon}{short_name}"), style));
    }
    Line::from(spans)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::app::{App, DataSnapshot};
    use crate::tui::log_buffer::LogRingBuffer;
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
        let backend = TestBackend::new(140, 20);
        let mut terminal = Terminal::new(backend).unwrap();

        let mut data = DataSnapshot::default();
        data.events = sample_events();
        let app = App::new(data, TuiConfig::default(), LogRingBuffer::new());
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
        assert!(text.contains("abc123"), "Should show SHA");
    }

    /// Extract plain text from a Line for assertion purposes.
    fn line_to_string(line: &Line<'_>) -> String {
        line.spans.iter().map(|s| s.content.as_ref()).collect()
    }

    #[test]
    fn platform_detail_github() {
        let theme = Theme::dark();
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
        assert_eq!(
            line_to_string(&platform_detail(&event, &theme)),
            "CI (push)"
        );
    }

    #[test]
    fn platform_detail_railway() {
        let theme = Theme::dark();
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
        assert_eq!(
            line_to_string(&platform_detail(&event, &theme)),
            "api / prod"
        );
    }

    #[test]
    fn platform_detail_vercel() {
        let theme = Theme::dark();
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
        assert_eq!(
            line_to_string(&platform_detail(&event, &theme)),
            "production → abc.vercel.app"
        );
    }

    #[test]
    fn pipeline_stages_render() {
        use pulsos_core::domain::deployment::JobSummary;

        let theme = Theme::dark();
        let event = DeploymentEvent {
            id: "run-99".into(),
            platform: Platform::GitHub,
            status: DeploymentStatus::Failed,
            commit_sha: Some("abc123".into()),
            branch: Some("main".into()),
            title: Some("CI".into()),
            actor: None,
            created_at: Utc::now(),
            updated_at: None,
            duration_secs: None,
            url: None,
            metadata: EventMetadata {
                workflow_name: Some("CI".into()),
                trigger_event: Some("push".into()),
                jobs: vec![
                    JobSummary {
                        name: "Build".into(),
                        status: DeploymentStatus::Success,
                    },
                    JobSummary {
                        name: "Test".into(),
                        status: DeploymentStatus::Success,
                    },
                    JobSummary {
                        name: "Deploy".into(),
                        status: DeploymentStatus::Failed,
                    },
                ],
                ..Default::default()
            },
            is_from_cache: false,
        };
        let line = platform_detail(&event, &theme);
        let text = line_to_string(&line);
        assert!(text.contains("Build"), "Should contain Build job");
        assert!(text.contains("Test"), "Should contain Test job");
        assert!(text.contains("Deploy"), "Should contain Deploy job");
        assert!(text.contains(" › "), "Should contain arrow separator");
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
