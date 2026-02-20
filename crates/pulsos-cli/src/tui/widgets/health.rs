//! Tab 3: Health & Metrics — split-pane with project list + detail panel.
//!
//! Left pane: project list with score and inline trend sparkline.
//! Right pane: detail panel with weight bars and history sparkline.
//!
//! Color thresholds per §4.2:
//!   ≥ 90 → status.success (green)
//!   ≥ 70 → status.warning (yellow)
//!   < 70 → status.failure (red)

use ratatui::{
    layout::{Constraint, Layout, Rect},
    prelude::Stylize,
    text::{Line, Span},
    widgets::{Block, Borders, Cell, Paragraph, Row, Sparkline, Table, TableState},
    Frame,
};

use crate::output::table::format_age;
use crate::tui::app::App;
use crate::tui::theme::Theme;
use crate::tui::widgets::{draw_search_bar, split_search_bar, status_spans};
use pulsos_core::domain::health::HealthBreakdown;

/// Draw the Health & Metrics tab.
pub fn draw(frame: &mut Frame, area: Rect, app: &App, theme: &Theme) {
    let (search_area, area) = split_search_bar(area, app);
    if let Some(sa) = search_area {
        draw_search_bar(frame, sa, app, theme);
    }

    if app.data.health_scores.is_empty() {
        let msg = Paragraph::new("No health data available.").style(theme.t7());
        frame.render_widget(msg, area);
        return;
    }

    let panes = Layout::horizontal([Constraint::Length(34), Constraint::Min(0)]).split(area);

    draw_project_list(frame, panes[0], app, theme);
    draw_detail_panel(frame, panes[1], app, theme);
}

/// Left pane — project list table with score and inline trend sparkline.
fn draw_project_list(frame: &mut Frame, area: Rect, app: &App, theme: &Theme) {
    let header_cells = ["Project", "Score", "Trend"]
        .iter()
        .map(|h| Cell::from(*h).style(theme.t4()));
    let header = Row::new(header_cells).height(1);

    let rows: Vec<Row> = app
        .data
        .health_scores
        .iter()
        .enumerate()
        .map(|(i, (name, score))| {
            let is_selected = i == app.selected_row;
            let row_style = if is_selected {
                theme.selected_row()
            } else {
                ratatui::style::Style::default()
            };

            let score_color = theme.health_color(*score);
            let score_style = ratatui::style::Style::new().fg(score_color).bold();

            let trend = render_sparkline_text(name, &app.data.health_history);
            let status_label = health_status_label(*score);

            // 2-row cell: row 1 = project name, row 2 = indented status label
            let name_cell = Cell::from(vec![
                Line::from(Span::styled(truncate_str(name, 18), theme.t5())),
                Line::from(vec![
                    Span::raw("  "),
                    Span::styled(status_label, ratatui::style::Style::new().fg(score_color)),
                ]),
            ]);

            Row::new(vec![
                name_cell,
                Cell::from(Span::styled(format!("{score:>3}"), score_style)),
                Cell::from(Span::styled(trend, theme.t8())),
            ])
            .height(2)
            .style(row_style)
        })
        .collect();

    let widths = [
        Constraint::Length(18), // Project
        Constraint::Length(5),  // Score
        Constraint::Length(10), // Trend
    ];

    let table = Table::new(rows, widths)
        .header(header)
        .block(Block::default().borders(Borders::NONE))
        .row_highlight_style(theme.selected_row())
        .highlight_symbol("▶ ");

    let mut table_state = TableState::default();
    if !app.data.health_scores.is_empty() {
        table_state.select(Some(
            app.selected_row
                .min(app.data.health_scores.len().saturating_sub(1)),
        ));
    }

    frame.render_stateful_widget(table, area, &mut table_state);
}

/// Right pane — detail panel with project header, weight bars, and sparkline.
fn draw_detail_panel(frame: &mut Frame, area: Rect, app: &App, theme: &Theme) {
    let block = Block::default().borders(Borders::LEFT).border_style(theme.panel_border());
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let selected = app
        .selected_row
        .min(app.data.health_scores.len().saturating_sub(1));

    let (project_name, score) = match app.data.health_scores.get(selected) {
        Some((name, score)) => (name.as_str(), *score),
        None => return,
    };

    let breakdown = app
        .data
        .health_breakdowns
        .iter()
        .find(|(n, _)| n == project_name)
        .map(|(_, bd)| bd);

    let sections = Layout::vertical([
        Constraint::Length(2), // Header
        Constraint::Length(5), // Weight bars
        Constraint::Length(6), // Recent Events
        Constraint::Min(3),   // Sparkline
    ])
    .split(inner);

    // Section 1: Header
    let score_color = theme.health_color(score);
    let header_line = Line::from(vec![
        Span::styled(format!(" {project_name}"), theme.t5()),
        Span::styled("  ", theme.t8()),
        Span::styled(
            format!("{score}/100"),
            ratatui::style::Style::new().fg(score_color).bold(),
        ),
        Span::styled(
            format!("  {}", health_status_label(score)),
            ratatui::style::Style::new().fg(score_color),
        ),
    ]);
    frame.render_widget(Paragraph::new(header_line), sections[0]);

    // Section 2: Platform weight bars
    draw_weight_bars(frame, sections[1], breakdown, theme);

    // Section 3: Recent Events
    draw_recent_events(frame, sections[2], project_name, app, theme);

    // Section 4: History sparkline
    draw_history_sparkline(frame, sections[3], project_name, app, theme);
}

/// Render per-platform weight bars with accent colors.
fn draw_weight_bars(
    frame: &mut Frame,
    area: Rect,
    breakdown: Option<&HealthBreakdown>,
    theme: &Theme,
) {
    let title = Line::from(Span::styled(" PLATFORM WEIGHTS", theme.t4()));
    let mut lines = vec![title];

    if let Some(bd) = breakdown {
        if let Some(score) = bd.github_score {
            lines.push(render_bar_line("GH", bd.github_weight, score, theme.platform_gh, theme));
        }
        if let Some(score) = bd.railway_score {
            lines.push(render_bar_line("RW", bd.railway_weight, score, theme.platform_rw, theme));
        }
        if let Some(score) = bd.vercel_score {
            lines.push(render_bar_line("VC", bd.vercel_weight, score, theme.platform_vc, theme));
        }
    } else {
        lines.push(Line::from(Span::styled(" No breakdown data", theme.t8())));
    }

    frame.render_widget(Paragraph::new(lines), area);
}

/// Render a single weight bar line: `GH 40% ████████░░░░ 70%`
fn render_bar_line<'a>(
    label: &'a str,
    weight: u8,
    score: u8,
    accent: ratatui::style::Color,
    theme: &'a Theme,
) -> Line<'a> {
    let bar_width = 12;
    let filled = ((score as usize) * bar_width / 100).min(bar_width);
    let empty = bar_width - filled;

    let bar_filled: String = "█".repeat(filled);
    let bar_empty: String = "░".repeat(empty);

    Line::from(vec![
        Span::styled(format!(" {label}"), ratatui::style::Style::new().fg(accent).bold()),
        Span::styled(format!(" {:>2}%", weight), theme.t8()),
        Span::raw(" "),
        Span::styled(bar_filled, ratatui::style::Style::new().fg(accent)),
        Span::styled(bar_empty, theme.t9()),
        Span::raw(" "),
        Span::styled(
            format!("{score}%"),
            ratatui::style::Style::new().fg(theme.health_color(score)),
        ),
    ])
}

/// Render the "RECENT EVENTS" section listing correlated events for this project.
fn draw_recent_events(
    frame: &mut Frame,
    area: Rect,
    project_name: &str,
    app: &App,
    theme: &Theme,
) {
    let title = Line::from(Span::styled(" RECENT EVENTS", theme.t4()));
    let mut lines = vec![title];

    let matching: Vec<_> = app
        .data
        .correlated
        .iter()
        .filter(|c| c.project_name.as_deref() == Some(project_name))
        .take(4)
        .collect();

    if matching.is_empty() {
        lines.push(Line::from(Span::styled(
            " No events for this project",
            theme.t8(),
        )));
    } else {
        for corr in matching {
            let age = format_age(corr.timestamp);
            // Determine overall status from the first available platform event
            let overall_status = corr
                .github
                .as_ref()
                .map(|e| &e.status)
                .or_else(|| corr.railway.as_ref().map(|e| &e.status))
                .or_else(|| corr.vercel.as_ref().map(|e| &e.status));
            let (sym, _label, style) = match overall_status {
                Some(s) => status_spans(s, theme),
                None => ("? ".into(), "unknown".into(), theme.neutral()),
            };
            let msg = corr
                .github
                .as_ref()
                .and_then(|e| e.title.as_deref())
                .or_else(|| corr.railway.as_ref().and_then(|e| e.title.as_deref()))
                .or_else(|| corr.vercel.as_ref().and_then(|e| e.title.as_deref()))
                .unwrap_or("-");
            let max_msg = area.width.saturating_sub(16) as usize;
            lines.push(Line::from(vec![
                Span::styled(format!(" {sym}"), style),
                Span::styled(format!(" {age:<8}"), theme.t8()),
                Span::styled(truncate_str(msg, max_msg), theme.t6()),
            ]));
        }
    }
    frame.render_widget(Paragraph::new(lines), area);
}

/// Render the sparkline for the selected project's health history.
fn draw_history_sparkline(
    frame: &mut Frame,
    area: Rect,
    project_name: &str,
    app: &App,
    theme: &Theme,
) {
    let history = app
        .data
        .health_history
        .iter()
        .find(|(n, _)| n == project_name)
        .map(|(_, data)| data);

    if let Some(data) = history {
        let data_u64: Vec<u64> = data.iter().map(|&v| v as u64).collect();

        let sparkline = Sparkline::default()
            .block(
                Block::default()
                    .borders(Borders::TOP)
                    .border_style(theme.panel_border())
                    .title(Span::styled(
                        format!(
                            " History (last {} points) ",
                            data_u64.len()
                        ),
                        theme.t6(),
                    )),
            )
            .data(&data_u64)
            .max(100)
            .style(ratatui::style::Style::new().fg(theme.status_active));

        frame.render_widget(sparkline, area);
    } else {
        let msg = Paragraph::new(" No history for selected project.")
            .style(theme.t8())
            .block(
                Block::default()
                    .borders(Borders::TOP)
                    .border_style(theme.panel_border()),
            );
        frame.render_widget(msg, area);
    }
}

/// Generate an inline text sparkline from health history.
fn render_sparkline_text(project: &str, history: &[(String, Vec<u8>)]) -> String {
    const BLOCKS: [char; 8] = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];

    let data = history
        .iter()
        .find(|(n, _)| n == project)
        .map(|(_, d)| d);

    match data {
        Some(points) if !points.is_empty() => {
            let last_8: &[u8] = if points.len() > 8 {
                &points[points.len() - 8..]
            } else {
                points
            };
            last_8
                .iter()
                .map(|&v| {
                    let idx = ((v as usize) * 7 / 100).min(7);
                    BLOCKS[idx]
                })
                .collect()
        }
        _ => "—".to_string(),
    }
}

/// Truncate a string to max chars.
fn truncate_str(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let mut out: String = s.chars().take(max.saturating_sub(1)).collect();
        out.push('…');
        out
    }
}

/// Map a health score to a human-readable status label.
/// Thresholds per §4.2: ≥90 Healthy, ≥70 Degraded, <70 Critical.
fn health_status_label(score: u8) -> &'static str {
    if score >= 90 {
        "Healthy"
    } else if score >= 70 {
        "Degraded"
    } else {
        "Critical"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::app::{App, DataSnapshot};
    use crate::tui::log_buffer::LogRingBuffer;
    use crate::tui::theme::Theme;
    use pulsos_core::config::types::TuiConfig;
    use pulsos_core::domain::health::HealthBreakdown;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    fn test_app_with_health() -> App {
        let mut data = DataSnapshot::default();
        data.health_scores = vec![
            ("my-saas".into(), 95),
            ("api-server".into(), 75),
            ("frontend".into(), 50),
        ];
        data.health_breakdowns = vec![
            (
                "my-saas".into(),
                HealthBreakdown {
                    total: 95,
                    github_score: Some(90),
                    railway_score: Some(100),
                    vercel_score: Some(100),
                    github_weight: 40,
                    railway_weight: 35,
                    vercel_weight: 25,
                },
            ),
            (
                "api-server".into(),
                HealthBreakdown {
                    total: 75,
                    github_score: Some(70),
                    railway_score: Some(80),
                    vercel_score: None,
                    github_weight: 53,
                    railway_weight: 47,
                    vercel_weight: 0,
                },
            ),
        ];
        data.health_history = vec![
            ("my-saas".into(), vec![80, 85, 90, 92, 95]),
            ("api-server".into(), vec![70, 60, 55, 60, 65]),
        ];
        let mut config = TuiConfig::default();
        config.show_sparklines = true;
        App::new(data, config, LogRingBuffer::new())
    }

    #[test]
    fn health_tab_renders_without_panic() {
        let backend = TestBackend::new(100, 20);
        let mut terminal = Terminal::new(backend).unwrap();

        let app = test_app_with_health();
        let theme = Theme::dark();

        terminal
            .draw(|frame| {
                draw(frame, frame.area(), &app, &theme);
            })
            .unwrap();

        let buf = terminal.backend().buffer().clone();
        let text = buffer_to_string(&buf);
        assert!(text.contains("my-saas"), "Should show project name");
        assert!(text.contains("95"), "Should show score");
        assert!(text.contains("PLATFORM WEIGHTS"), "Should show weight bar section");
        assert!(
            text.contains("Healthy") || text.contains("Degraded") || text.contains("Critical"),
            "Should show status label in 2-row items"
        );
    }

    #[test]
    fn health_tab_renders_empty() {
        let backend = TestBackend::new(80, 10);
        let mut terminal = Terminal::new(backend).unwrap();

        let app = App::new(DataSnapshot::default(), TuiConfig::default(), LogRingBuffer::new());
        let theme = Theme::dark();

        terminal
            .draw(|frame| {
                draw(frame, frame.area(), &app, &theme);
            })
            .unwrap();

        let buf = terminal.backend().buffer().clone();
        let text = buffer_to_string(&buf);
        assert!(text.contains("No health data"), "Should show empty message");
    }

    #[test]
    fn health_status_labels() {
        assert_eq!(health_status_label(100), "Healthy");
        assert_eq!(health_status_label(90), "Healthy");
        assert_eq!(health_status_label(89), "Degraded");
        assert_eq!(health_status_label(70), "Degraded");
        assert_eq!(health_status_label(69), "Critical");
        assert_eq!(health_status_label(0), "Critical");
    }

    #[test]
    fn sparkline_text_rendering() {
        let history = vec![("proj".to_string(), vec![0, 50, 100])];
        let text = render_sparkline_text("proj", &history);
        assert_eq!(text.chars().count(), 3);
        assert!(text.starts_with('▁'));
        assert!(text.ends_with('█'));

        let empty = render_sparkline_text("nonexistent", &history);
        assert_eq!(empty, "—");
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
