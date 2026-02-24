//! Tab 2: Platform Details — per-platform deployment event details.
//!
//! Shows all deployment events with platform-specific detail columns:
//! - GitHub: workflow_name + trigger_event
//! - Railway: service_name / environment_name
//! - Vercel: deploy_target + preview_url

use ratatui::{
    layout::Rect,
    layout::{Constraint, Layout},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Cell, Clear, Paragraph, Row, Table, TableState, Wrap},
    Frame,
};

use crate::output::table::{format_age, format_duration};
use crate::tui::app::{
    App, DetailsFocus, PlatformDetailsState, PlatformSubtab, RightContent, TreeItemKind,
};
use crate::tui::theme::Theme;
use crate::tui::widgets::{draw_search_bar, split_search_bar, status_spans};
use pulsos_core::domain::deployment::{DeploymentEvent, JobSummary, Platform};

/// Draw the Platform Details table.
pub fn draw(frame: &mut Frame, area: Rect, app: &App, theme: &Theme) {
    let (search_area, area) = split_search_bar(area, app);
    if let Some(sa) = search_area {
        draw_search_bar(frame, sa, app, theme);
    }

    let details_mode = if app.platform_subtab == PlatformSubtab::GitHub {
        app.details_state()
    } else {
        None
    };
    let show_dropdown = details_mode.map(|d| d.show_logs).unwrap_or(false);

    let sections = Layout::vertical([Constraint::Length(1), Constraint::Min(0)]).split(area);
    draw_subtab_bar(frame, sections[0], app, theme);

    match app.platform_subtab {
        PlatformSubtab::GitHub => draw_github_table(frame, sections[1], app, theme),
        PlatformSubtab::Railway => draw_railway_table(frame, sections[1], app, theme),
        PlatformSubtab::Vercel => draw_vercel_table(frame, sections[1], app, theme),
    }

    // Overlay: draw AFTER the table so it appears on top.
    if show_dropdown {
        if let Some(details) = details_mode {
            let overlay_rect = dropdown_rect(sections[1], app.selected_row);
            draw_dropdown_overlay(frame, overlay_rect, app, details, theme);
        }
    }
}

/// Compute the dropdown overlay rect positioned just below the selected row.
/// Flips above if there isn't enough space below.
fn dropdown_rect(table_area: Rect, selected_row: usize) -> Rect {
    const HEIGHT: u16 = 20;
    // Header occupies the first row; data rows start at y+1.
    let row_y = table_area
        .y
        .saturating_add(1)
        .saturating_add(selected_row as u16);
    let below_y = row_y.saturating_add(1);
    let space_below = table_area.bottom().saturating_sub(below_y);
    let y = if space_below >= HEIGHT {
        below_y
    } else {
        row_y.saturating_sub(HEIGHT)
    };
    let h = HEIGHT.min(table_area.bottom().saturating_sub(y));
    Rect {
        x: table_area.x,
        y,
        width: table_area.width,
        height: h,
    }
}

/// Draw the CI tree dropdown overlay.
fn draw_dropdown_overlay(
    frame: &mut Frame,
    area: Rect,
    app: &App,
    details: &PlatformDetailsState,
    theme: &Theme,
) {
    frame.render_widget(Clear, area);

    let title_text = app
        .data
        .events
        .iter()
        .find(|e| e.id == details.anchor_event_id)
        .and_then(|e| e.metadata.workflow_name.as_deref())
        .unwrap_or(details.anchor_event_id.as_str());

    let title = format!(" {title_text} ");
    let has_details = details.focus == DetailsFocus::RightPanel;

    let outer_block = Block::default()
        .borders(Borders::ALL)
        .border_style(theme.panel_border_focus())
        .title(Span::styled(title, theme.t4()));
    let inner = outer_block.inner(area);
    frame.render_widget(outer_block, area);

    if has_details && inner.width > 20 {
        let cols = Layout::horizontal([Constraint::Percentage(45), Constraint::Percentage(55)])
            .split(inner);
        let tree_lines = render_tree_lines(app, theme, inner.height);
        frame.render_widget(Paragraph::new(Text::from(tree_lines)), cols[0]);
        draw_step_details(frame, cols[1], details, theme);
    } else {
        let tree_lines = render_tree_lines(app, theme, inner.height);
        frame.render_widget(Paragraph::new(Text::from(tree_lines)), inner);
    }
}

/// Render the right half of the dropdown (step/job/run detail), with a left-border divider.
fn draw_step_details(frame: &mut Frame, area: Rect, details: &PlatformDetailsState, theme: &Theme) {
    let block = Block::default()
        .borders(Borders::LEFT)
        .border_style(theme.panel_border());
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let lines: Vec<Line<'static>> = match &details.right_content {
        RightContent::Summary { lines: body } => body
            .iter()
            .map(|l| Line::from(Span::styled(l.clone(), theme.t6())))
            .collect(),
        RightContent::Error { message } => {
            vec![Line::from(Span::styled(message.clone(), theme.failure()))]
        }
    };
    frame.render_widget(
        Paragraph::new(Text::from(lines))
            .wrap(Wrap { trim: false })
            .scroll((details.right_scroll.min(u16::MAX as usize) as u16, 0)),
        inner,
    );
}

fn draw_subtab_bar(frame: &mut Frame, area: Rect, app: &App, theme: &Theme) {
    let subtabs = [
        PlatformSubtab::GitHub,
        PlatformSubtab::Railway,
        PlatformSubtab::Vercel,
    ];
    let mut spans = Vec::new();
    for (idx, subtab) in subtabs.iter().enumerate() {
        if idx > 0 {
            spans.push(Span::styled(" │ ", theme.t8()));
        }
        let style = if *subtab == app.platform_subtab {
            theme.tab_active()
        } else {
            theme.tab_inactive()
        };
        spans.push(Span::styled(subtab.short_label(), style));
    }
    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

fn draw_github_table(frame: &mut Frame, area: Rect, app: &App, theme: &Theme) {
    let header_cells = [
        "Status", "Workflow", "Repo", "SHA", "Branch", "Actor", "Age", "Dur", "Detail",
    ]
    .iter()
    .map(|h| Cell::from(*h).style(theme.t4()));
    let header = Row::new(header_cells).height(1);

    let filtered_indices = app.platform_filtered_event_indices();
    let filtered_events: Vec<&DeploymentEvent> = filtered_indices
        .iter()
        .filter_map(|idx| app.data.events.get(*idx))
        .collect();

    let selected_index = if filtered_events.is_empty() {
        None
    } else {
        Some(
            app.selected_row
                .min(filtered_events.len().saturating_sub(1)),
        )
    };

    let rows: Vec<Row> = filtered_events
        .iter()
        .map(|event| {
            let (sym, label, style) = status_spans(&event.status, theme);
            let status_cell = Cell::from(Line::from(vec![
                Span::styled(sym, style),
                Span::styled(label, style),
            ]));

            let title = event
                .metadata
                .workflow_name
                .as_deref()
                .or(event.title.as_deref())
                .unwrap_or_else(|| &event.id[..event.id.len().min(12)]);
            let repo_short = event
                .metadata
                .source_id
                .as_deref()
                .and_then(|s| s.split('/').next_back())
                .unwrap_or("-");
            let sha = short_sha(event.commit_sha.as_deref());
            let branch = event.branch.as_deref().unwrap_or("-");
            let actor = event.actor.as_deref().unwrap_or("-");
            let age = format_age(event.created_at);
            let duration = event
                .duration_secs
                .map(format_duration)
                .unwrap_or_else(|| "-".into());

            Row::new(vec![
                status_cell,
                Cell::from(Span::styled(title.to_string(), theme.t6())),
                Cell::from(Span::styled(repo_short.to_string(), theme.t7())),
                Cell::from(Span::styled(sha, theme.active())),
                Cell::from(Span::styled(branch.to_string(), theme.t6())),
                Cell::from(Span::styled(actor.to_string(), theme.t6())),
                Cell::from(Span::styled(age, theme.t8())),
                Cell::from(Span::styled(duration, theme.t8())),
                Cell::from(platform_detail(event, theme)),
            ])
        })
        .collect();

    let widths = [
        Constraint::Length(12),
        Constraint::Length(22),
        Constraint::Length(18),
        Constraint::Length(9),
        Constraint::Length(12),
        Constraint::Length(12),
        Constraint::Length(7),
        Constraint::Length(8),
        Constraint::Min(28),
    ];

    let table = Table::new(rows, widths)
        .header(header)
        .block(Block::default().borders(Borders::NONE))
        .row_highlight_style(theme.selected_row())
        .highlight_symbol("▶ ");

    let mut table_state = TableState::default();
    if let Some(selected) = selected_index {
        table_state.select(Some(selected));
    }
    frame.render_stateful_widget(table, area, &mut table_state);
}

fn draw_railway_table(frame: &mut Frame, area: Rect, app: &App, theme: &Theme) {
    let header_cells = [
        "Status",
        "Service",
        "Env",
        "Commit",
        "Branch",
        "Deployment URL",
        "Age",
        "Dur",
        "Message",
    ]
    .iter()
    .map(|h| Cell::from(*h).style(theme.t4()));
    let header = Row::new(header_cells).height(1);

    let rows_data = app.platform_latest_rows();
    let rows: Vec<Row> = rows_data
        .iter()
        .filter_map(|row| app.data.events.get(row.event_idx))
        .map(|event| {
            let (sym, label, style) = status_spans(&event.status, theme);
            let status_cell = Cell::from(Line::from(vec![
                Span::styled(sym, style),
                Span::styled(label, style),
            ]));
            let source_id = event.metadata.source_id.as_deref().unwrap_or_default();
            let source_parts: Vec<&str> = source_id.split(':').collect();
            let service = event
                .metadata
                .service_name
                .as_deref()
                .or_else(|| source_parts.get(1).copied())
                .unwrap_or("-");
            let environment = event
                .metadata
                .environment_name
                .as_deref()
                .or_else(|| source_parts.get(2).copied())
                .unwrap_or("-");
            let branch = event.branch.as_deref().unwrap_or("-");
            let deploy_url = event.url.as_deref().unwrap_or("-");
            let age = format_age(event.created_at);
            let duration = event
                .duration_secs
                .map(format_duration)
                .unwrap_or_else(|| "-".into());
            let message = event.title.as_deref().unwrap_or("-");

            Row::new(vec![
                status_cell,
                Cell::from(Span::styled(service.to_string(), theme.t6())),
                Cell::from(Span::styled(environment.to_string(), theme.t6())),
                Cell::from(Span::styled(
                    short_sha(event.commit_sha.as_deref()),
                    theme.active(),
                )),
                Cell::from(Span::styled(branch.to_string(), theme.t6())),
                Cell::from(Span::styled(deploy_url.to_string(), theme.active())),
                Cell::from(Span::styled(age, theme.t8())),
                Cell::from(Span::styled(duration, theme.t8())),
                Cell::from(Span::styled(message.to_string(), theme.t6())),
            ])
        })
        .collect();

    let widths = [
        Constraint::Length(12),
        Constraint::Length(16),
        Constraint::Length(12),
        Constraint::Length(9),
        Constraint::Length(12),
        Constraint::Min(26),
        Constraint::Length(7),
        Constraint::Length(8),
        Constraint::Length(24),
    ];

    let table = Table::new(rows, widths)
        .header(header)
        .block(Block::default().borders(Borders::NONE))
        .row_highlight_style(theme.selected_row())
        .highlight_symbol("▶ ");
    let mut table_state = TableState::default();
    if !rows_data.is_empty() {
        table_state.select(Some(
            app.selected_row.min(rows_data.len().saturating_sub(1)),
        ));
    }
    frame.render_stateful_widget(table, area, &mut table_state);
}

fn draw_vercel_table(frame: &mut Frame, area: Rect, app: &App, theme: &Theme) {
    let header_cells = [
        "Status",
        "Project",
        "Target",
        "Deployment URL",
        "Commit",
        "Branch",
        "Age",
        "Dur",
        "Message",
    ]
    .iter()
    .map(|h| Cell::from(*h).style(theme.t4()));
    let header = Row::new(header_cells).height(1);

    let rows_data = app.platform_latest_rows();
    let rows: Vec<Row> = rows_data
        .iter()
        .filter_map(|row| app.data.events.get(row.event_idx))
        .map(|event| {
            let (sym, label, style) = status_spans(&event.status, theme);
            let status_cell = Cell::from(Line::from(vec![
                Span::styled(sym, style),
                Span::styled(label, style),
            ]));
            let project = event.metadata.source_id.as_deref().unwrap_or("-");
            let target = event.metadata.deploy_target.as_deref().unwrap_or("-");
            let deploy_url = event
                .url
                .as_deref()
                .or(event.metadata.preview_url.as_deref())
                .unwrap_or("-");
            let branch = event.branch.as_deref().unwrap_or("-");
            let age = format_age(event.created_at);
            let duration = event
                .duration_secs
                .map(format_duration)
                .unwrap_or_else(|| "-".into());
            let message = event.title.as_deref().unwrap_or("-");

            Row::new(vec![
                status_cell,
                Cell::from(Span::styled(project.to_string(), theme.t6())),
                Cell::from(Span::styled(target.to_string(), theme.t6())),
                Cell::from(Span::styled(deploy_url.to_string(), theme.active())),
                Cell::from(Span::styled(
                    short_sha(event.commit_sha.as_deref()),
                    theme.active(),
                )),
                Cell::from(Span::styled(branch.to_string(), theme.t6())),
                Cell::from(Span::styled(age, theme.t8())),
                Cell::from(Span::styled(duration, theme.t8())),
                Cell::from(Span::styled(message.to_string(), theme.t6())),
            ])
        })
        .collect();

    let widths = [
        Constraint::Length(12),
        Constraint::Length(18),
        Constraint::Length(11),
        Constraint::Min(26),
        Constraint::Length(9),
        Constraint::Length(12),
        Constraint::Length(7),
        Constraint::Length(8),
        Constraint::Length(24),
    ];

    let table = Table::new(rows, widths)
        .header(header)
        .block(Block::default().borders(Borders::NONE))
        .row_highlight_style(theme.selected_row())
        .highlight_symbol("▶ ");
    let mut table_state = TableState::default();
    if !rows_data.is_empty() {
        table_state.select(Some(
            app.selected_row.min(rows_data.len().saturating_sub(1)),
        ));
    }
    frame.render_stateful_widget(table, area, &mut table_state);
}

fn short_sha(sha: Option<&str>) -> String {
    match sha {
        Some(s) if s.len() > 7 => s[..7].to_string(),
        Some(s) => s.to_string(),
        None => "-".to_string(),
    }
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

fn render_tree_lines(app: &App, theme: &Theme, available_height: u16) -> Vec<Line<'static>> {
    let Some(details) = app.details_state() else {
        return vec![Line::from(Span::styled("-", theme.t8()))];
    };
    if details.tree_items.is_empty() {
        return vec![Line::from(Span::styled("No workflow tree", theme.t8()))];
    }

    let max_lines = available_height.max(1) as usize;
    let last_idx = details.tree_items.len().saturating_sub(1);
    let cursor = details.tree_cursor.min(last_idx);
    let mut start = details.tree_scroll.min(last_idx);
    if cursor < start {
        start = cursor;
    }
    if cursor >= start + max_lines {
        start = cursor.saturating_sub(max_lines.saturating_sub(1));
    }
    let end = (start + max_lines).min(details.tree_items.len());

    details.tree_items[start..end]
        .iter()
        .enumerate()
        .map(|(offset, item)| {
            let absolute_idx = start + offset;
            let is_cursor = absolute_idx == cursor;
            let indent = "  ".repeat(item.depth as usize);
            let marker = match item.kind {
                TreeItemKind::RunHeader => "▾",
                TreeItemKind::Job { job_id, .. } => {
                    if details.expanded_jobs.contains(&job_id) {
                        "▾"
                    } else {
                        "▸"
                    }
                }
                TreeItemKind::Step { .. } => "•",
            };
            let (sym, _label, status_style) = status_spans(&item.status, theme);
            let text_style = if is_cursor && details.focus == DetailsFocus::LeftTree {
                theme.active()
            } else {
                theme.t6()
            };

            Line::from(vec![
                Span::styled(format!("{indent}{marker} "), theme.t8()),
                Span::styled(sym, status_style),
                Span::styled(item.label.clone(), text_style),
            ])
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::app::{App, DataSnapshot, PlatformSubtab};
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

        let data = DataSnapshot {
            events: sample_events(),
            ..Default::default()
        };
        let app = App::new(data, TuiConfig::default(), LogRingBuffer::new());
        let theme = Theme::dark();

        terminal
            .draw(|frame| {
                draw(frame, frame.area(), &app, &theme);
            })
            .unwrap();

        let buf = terminal.backend().buffer().clone();
        let text = buffer_to_string(&buf);
        assert!(text.contains("GH"), "Should show GH subtab");
        assert!(text.contains("RW"), "Should show RW subtab");
        assert!(text.contains("VC"), "Should show VC subtab");
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

    #[test]
    fn railway_subtab_renders_provider_specific_columns() {
        let backend = TestBackend::new(160, 20);
        let mut terminal = Terminal::new(backend).unwrap();
        let data = DataSnapshot {
            events: sample_events(),
            ..Default::default()
        };
        let mut app = App::new(data, TuiConfig::default(), LogRingBuffer::new());
        app.platform_subtab = PlatformSubtab::Railway;
        let theme = Theme::dark();

        terminal
            .draw(|frame| {
                draw(frame, frame.area(), &app, &theme);
            })
            .unwrap();

        let text = buffer_to_string(&terminal.backend().buffer().clone());
        assert!(text.contains("Service"));
        assert!(text.contains("Env"));
        assert!(text.contains("Deployment URL"));
    }

    #[test]
    fn vercel_subtab_renders_provider_specific_columns() {
        let backend = TestBackend::new(160, 20);
        let mut terminal = Terminal::new(backend).unwrap();
        let data = DataSnapshot {
            events: sample_events(),
            ..Default::default()
        };
        let mut app = App::new(data, TuiConfig::default(), LogRingBuffer::new());
        app.platform_subtab = PlatformSubtab::Vercel;
        let theme = Theme::dark();

        terminal
            .draw(|frame| {
                draw(frame, frame.area(), &app, &theme);
            })
            .unwrap();

        let text = buffer_to_string(&terminal.backend().buffer().clone());
        assert!(text.contains("Project"));
        assert!(text.contains("Target"));
        assert!(text.contains("Deployment URL"));
    }

    #[test]
    fn github_tree_includes_step_breakdown() {
        use pulsos_core::domain::deployment::{JobDetail, JobStepSummary};

        let theme = Theme::dark();
        let event = DeploymentEvent {
            id: "run-42".into(),
            platform: Platform::GitHub,
            status: DeploymentStatus::Failed,
            commit_sha: Some("abc123".into()),
            branch: Some("main".into()),
            title: Some("CI".into()),
            actor: Some("vivallo".into()),
            created_at: Utc::now(),
            updated_at: None,
            duration_secs: None,
            url: None,
            metadata: EventMetadata {
                workflow_name: Some("CI".into()),
                trigger_event: Some("push".into()),
                job_details: vec![JobDetail {
                    job_id: Some(7001),
                    html_url: Some("https://github.com/owner/repo/actions/runs/42/job/7001".into()),
                    name: "Build and Test".into(),
                    status: DeploymentStatus::Failed,
                    steps: vec![
                        JobStepSummary {
                            number: 1,
                            name: "Checkout".into(),
                            status: DeploymentStatus::Success,
                            duration_secs: Some(3),
                            started_at: None,
                            completed_at: None,
                        },
                        JobStepSummary {
                            number: 2,
                            name: "Run tests".into(),
                            status: DeploymentStatus::Failed,
                            duration_secs: Some(12),
                            started_at: None,
                            completed_at: None,
                        },
                    ],
                }],
                ..Default::default()
            },
            is_from_cache: false,
        };
        let data = DataSnapshot {
            events: vec![event],
            ..Default::default()
        };
        let mut app = App::new(data, TuiConfig::default(), LogRingBuffer::new());
        app.active_tab = crate::tui::app::Tab::Platform;
        app.selected_row = 0;
        app.toggle_platform_logs_panel();
        assert!(app.platform_details_active());
        app.details_move_tree_cursor(1);
        app.details_toggle_or_open_right();

        let lines = render_tree_lines(&app, &theme, 20);
        let text = lines
            .iter()
            .map(line_to_string)
            .collect::<Vec<_>>()
            .join(" ");
        assert!(text.contains("Build and Test"), "Should contain job name");
        assert!(text.contains("1. Checkout"), "Should contain first step");
        assert!(text.contains("2. Run tests"), "Should contain second step");
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
