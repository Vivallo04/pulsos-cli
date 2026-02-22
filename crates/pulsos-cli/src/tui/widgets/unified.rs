//! Tab 1: Unified Overview — correlated events across all platforms.
//!
//! Columns: Project(16) | SHA(9) | Message(min 24) | GitHub CI(14) | Railway(12) | Vercel(10) | Branch(18) | Age(7)

use pulsos_core::domain::analytics::{DoraMetrics, DoraRating};
use ratatui::{
    layout::{Constraint, Layout, Rect},
    text::{Line, Span},
    widgets::{Block, Borders, Cell, Paragraph, Row, Table, TableState},
    Frame,
};

use crate::output::table::{format_age, format_duration};
use crate::tui::app::{App, UnifiedSort};
use crate::tui::theme::Theme;
use crate::tui::widgets::{draw_search_bar, split_search_bar, status_spans};

/// Draw the Unified Overview table, optionally with a DORA banner above it.
pub fn draw(frame: &mut Frame, area: Rect, app: &App, theme: &Theme) {
    let (search_area, area) = split_search_bar(area, app);
    if let Some(sa) = search_area {
        draw_search_bar(frame, sa, app, theme);
    }

    // Conditionally reserve space for the DORA banner above the table.
    let has_dora = app.data.dora_metrics.deployment_frequency > 0
        || app.data.dora_metrics.lead_time_for_changes.is_some();

    let (dora_area, table_area) = if has_dora {
        let chunks = Layout::vertical([Constraint::Length(6), Constraint::Min(0)]).split(area);
        (Some(chunks[0]), chunks[1])
    } else {
        (None, area)
    };

    if let Some(da) = dora_area {
        draw_dora_banner(
            frame,
            da,
            &app.data.dora_metrics,
            app.data.dora_history_count,
            theme,
        );
    }

    // Build sorted view of correlated events for display.
    let mut sorted: Vec<&pulsos_core::domain::project::CorrelatedEvent> =
        app.data.correlated.iter().collect();
    if app.unified_sort == UnifiedSort::ByPlatform {
        // Stable sort by platform group; within each group, timestamp order is preserved.
        sorted.sort_by_key(|e| platform_sort_key(e));
    }

    // Header row (T4: bold + fg.subtle). Last column shows current sort mode.
    let age_header = match app.unified_sort {
        UnifiedSort::ByTime => "Age ↓",
        UnifiedSort::ByPlatform => "Plat ▾",
    };
    let header_cells = [
        "Project",
        "SHA",
        "Message",
        "GitHub CI",
        "Railway",
        "Vercel",
        "Branch",
        age_header,
    ]
    .into_iter()
    .map(|h| Cell::from(h).style(theme.t4()));
    let header = Row::new(header_cells).height(1);

    let rows: Vec<Row> = sorted
        .iter()
        .enumerate()
        .map(|(i, corr)| {
            let is_selected = i == app.selected_row;
            let row_style = if is_selected {
                theme.selected_row()
            } else {
                ratatui::style::Style::default()
            };

            // Project identifier: prefer config project_name, then platform titles, then SHA
            let project_name = corr
                .project_name
                .as_deref()
                .or_else(|| corr.vercel.as_ref().and_then(|e| e.title.as_deref()))
                .or_else(|| corr.railway.as_ref().and_then(|e| e.title.as_deref()))
                .or_else(|| corr.github.as_ref().and_then(|e| e.title.as_deref()))
                .or_else(|| {
                    corr.commit_sha
                        .as_deref()
                        .map(|s| if s.len() > 8 { &s[..8] } else { s })
                })
                .unwrap_or("-")
                .to_string();

            // SHA cell: first 7 chars, blue accent
            let sha = corr
                .commit_sha
                .as_deref()
                .map(|s| if s.len() > 7 { &s[..7] } else { s })
                .unwrap_or("-");

            // Message cell: commit message from first available platform title
            let message = corr
                .github
                .as_ref()
                .and_then(|e| e.title.as_deref())
                .or_else(|| corr.railway.as_ref().and_then(|e| e.title.as_deref()))
                .or_else(|| corr.vercel.as_ref().and_then(|e| e.title.as_deref()))
                .unwrap_or("-");

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
                Cell::from(Span::styled(sha.to_string(), theme.active())),
                Cell::from(Span::styled(message.to_string(), theme.t7())),
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
        Constraint::Length(9),  // SHA
        Constraint::Min(24),    // Message
        Constraint::Length(14), // GitHub CI
        Constraint::Length(12), // Railway
        Constraint::Length(10), // Vercel
        Constraint::Length(18), // Branch
        Constraint::Length(7),  // Age
    ];

    let table = Table::new(rows, widths)
        .header(header)
        .block(Block::default().borders(Borders::NONE))
        .row_highlight_style(theme.selected_row())
        .highlight_symbol("▶ ");

    let mut table_state = TableState::default();
    if !sorted.is_empty() {
        let selected = app.selected_row.min(sorted.len().saturating_sub(1));
        table_state.select(Some(selected));
    }

    frame.render_stateful_widget(table, table_area, &mut table_state);
}

/// Sort key for "by platform" mode.
///
/// Priority: Railway (0) → Vercel (1) → GitHub-only (2) → Unmatched (3).
fn platform_sort_key(e: &pulsos_core::domain::project::CorrelatedEvent) -> u8 {
    if e.railway.is_some() {
        0
    } else if e.vercel.is_some() {
        1
    } else if e.github.is_some() {
        2
    } else {
        3
    }
}

/// Draw the DORA Metrics banner panel.
///
/// Layout (6 rows including block borders):
/// ```text
/// ┌─ DORA Metrics (N events) ──────────────────────────────────────────┐
/// │  🚀 VELOCITY                        🛡 STABILITY                   │
/// │  Deploy Freq: 12                    Change Failure: 4.1%  (Elite)  │
/// │  Lead Time: 4m 30s  (Elite)         Time to Restore: 12m  (High)   │
/// │  Window: 7d 4h  (200 events)                                        │
/// └────────────────────────────────────────────────────────────────────┘
/// ```
fn draw_dora_banner(
    frame: &mut Frame,
    area: Rect,
    metrics: &DoraMetrics,
    count: usize,
    theme: &Theme,
) {
    let rating_style = |r: DoraRating| match r {
        DoraRating::Elite => theme.success(),
        DoraRating::High | DoraRating::Medium => theme.warning(),
        DoraRating::Low => theme.failure(),
    };

    let lt_str = metrics
        .lead_time_for_changes
        .map(|d| format_duration(d.as_secs()))
        .unwrap_or_else(|| "—".into());

    let mttr_str = metrics
        .time_to_restore_service
        .map(|d| format_duration(d.as_secs()))
        .unwrap_or_else(|| "—".into());

    let cfr_str = format!("{:.1}%", metrics.change_failure_rate);
    let cfr_rating = metrics.cfr_rating();
    let lt_rating = metrics.lead_time_rating();

    // Row 1: emoji section headers
    let header_line = Line::from(vec![
        Span::styled("  🚀 VELOCITY", theme.t4()),
        Span::raw("                     "),
        Span::styled("🛡 STABILITY", theme.t4()),
    ]);

    // Row 2: Deploy Freq (left) + Change Failure Rate (right)
    let row2: Vec<Span> = vec![
        Span::styled("  Deploy Freq: ", theme.t7()),
        Span::styled(metrics.deployment_frequency.to_string(), theme.t5()),
        Span::raw("                    "),
        Span::styled("Change Failure: ", theme.t7()),
        Span::styled(cfr_str, rating_style(cfr_rating)),
        Span::styled(
            format!("  ({})", cfr_rating.label()),
            rating_style(cfr_rating),
        ),
    ];

    // Row 3: Lead Time (left) + MTTR (right)
    let mut row3: Vec<Span> = vec![
        Span::styled("  Lead Time: ", theme.t7()),
        Span::styled(lt_str, theme.t5()),
    ];
    if let Some(r) = lt_rating {
        row3.push(Span::styled(format!("  ({})", r.label()), rating_style(r)));
    }
    row3.push(Span::raw("          "));
    row3.push(Span::styled("Time to Restore: ", theme.t7()));
    row3.push(Span::styled(mttr_str, theme.t5()));

    // Row 4: window duration summary
    let window_str = match metrics.window_duration {
        Some(d) => format!(
            "  Window: {}  ({count} events)",
            format_duration(d.as_secs())
        ),
        None => format!("  {count} events tracked this session"),
    };
    let row4 = Line::from(Span::styled(window_str, theme.t9()));

    let text =
        ratatui::text::Text::from(vec![header_line, Line::from(row2), Line::from(row3), row4]);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(theme.panel_border())
        .title(Span::styled(
            format!(" DORA Metrics ({count} events) "),
            theme.t1(),
        ));

    frame.render_widget(Paragraph::new(text).block(block), area);
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
    use pulsos_core::domain::project::{Confidence, CorrelatedEvent};
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    fn sample_correlated_events() -> Vec<CorrelatedEvent> {
        vec![
            CorrelatedEvent {
                project_name: Some("my-saas".into()),
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
                project_name: None,
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
        let backend = TestBackend::new(140, 20);
        let mut terminal = Terminal::new(backend).unwrap();

        let mut data = DataSnapshot::default();
        data.correlated = sample_correlated_events();
        let app = App::new(data, TuiConfig::default(), LogRingBuffer::new());
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
        assert!(text.contains("abc123d"), "Should contain truncated SHA");
    }

    #[test]
    fn unified_tab_renders_empty_data() {
        let backend = TestBackend::new(100, 20);
        let mut terminal = Terminal::new(backend).unwrap();

        let data = DataSnapshot::default();
        let app = App::new(data, TuiConfig::default(), LogRingBuffer::new());
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
