//! Tab 3: Health & Metrics — health scores and sparklines per project.
//!
//! Shows per-project health scores (0-100) with color coding:
//! - Green (>= 80): Healthy
//! - Yellow (>= 50): Degraded
//! - Red (< 50): Critical
//!
//! Bottom section shows a sparkline of the last 20 data points for the
//! selected project (if sparklines are enabled in TuiConfig).

use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::Span,
    widgets::{Block, Borders, Cell, Row, Sparkline, Table, TableState},
    Frame,
};

use crate::tui::app::App;
use crate::tui::theme::Theme;

/// Draw the Health & Metrics tab.
pub fn draw(frame: &mut Frame, area: Rect, app: &App, theme: &Theme) {
    if app.data.health_scores.is_empty() {
        let msg = ratatui::widgets::Paragraph::new("No health data available.")
            .style(Style::default().fg(theme.muted));
        frame.render_widget(msg, area);
        return;
    }

    let show_sparklines = app.tui_config.show_sparklines && !app.data.health_history.is_empty();

    let chunks = if show_sparklines {
        Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Min(5),    // score table
                Constraint::Length(5), // sparkline area
            ])
            .split(area)
    } else {
        Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(5)])
            .split(area)
    };

    draw_score_table(frame, chunks[0], app, theme);

    if show_sparklines && chunks.len() > 1 {
        draw_sparklines(frame, chunks[1], app, theme);
    }
}

/// Render the project score table with color-coded values.
fn draw_score_table(frame: &mut Frame, area: Rect, app: &App, theme: &Theme) {
    let header_cells = ["Project", "Score", "Status"]
        .iter()
        .map(|h| Cell::from(*h).style(Style::default().add_modifier(Modifier::BOLD).fg(theme.fg)));
    let header = Row::new(header_cells).height(1);

    let rows: Vec<Row> = app
        .data
        .health_scores
        .iter()
        .enumerate()
        .map(|(i, (name, score))| {
            let is_selected = i == app.selected_row;
            let row_style = if is_selected {
                theme.highlight
            } else {
                Style::default()
            };

            let score_color = theme.health_color(*score);
            let status_label = health_status_label(*score);

            Row::new(vec![
                Cell::from(Span::raw(name.clone())),
                Cell::from(Span::styled(
                    format!("{score:>3}"),
                    Style::default().fg(score_color),
                )),
                Cell::from(Span::styled(status_label, Style::default().fg(score_color))),
            ])
            .style(row_style)
        })
        .collect();

    let widths = [
        Constraint::Length(24), // Project
        Constraint::Length(6),  // Score
        Constraint::Min(10),    // Status
    ];

    let table = Table::new(rows, widths)
        .header(header)
        .block(Block::default().borders(Borders::NONE))
        .row_highlight_style(theme.highlight);

    let mut table_state = TableState::default();
    if !app.data.health_scores.is_empty() {
        table_state.select(Some(
            app.selected_row
                .min(app.data.health_scores.len().saturating_sub(1)),
        ));
    }

    frame.render_stateful_widget(table, area, &mut table_state);
}

/// Render a sparkline for the currently selected project's health history.
fn draw_sparklines(frame: &mut Frame, area: Rect, app: &App, theme: &Theme) {
    let selected = app
        .selected_row
        .min(app.data.health_scores.len().saturating_sub(1));

    let project_name = app
        .data
        .health_scores
        .get(selected)
        .map(|(name, _)| name.as_str());

    let history = project_name.and_then(|name| {
        app.data
            .health_history
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, data)| data)
    });

    if let Some(data) = history {
        let data_u64: Vec<u64> = data.iter().map(|&v| v as u64).collect();

        let sparkline = Sparkline::default()
            .block(
                Block::default()
                    .borders(Borders::TOP)
                    .border_style(Style::default().fg(theme.border))
                    .title(Span::styled(
                        format!(
                            " {} (last {} points) ",
                            project_name.unwrap_or(""),
                            data_u64.len()
                        ),
                        Style::default().fg(theme.fg),
                    )),
            )
            .data(&data_u64)
            .max(100)
            .style(Style::default().fg(theme.in_progress));

        frame.render_widget(sparkline, area);
    } else {
        let msg = ratatui::widgets::Paragraph::new("No history for selected project.")
            .style(Style::default().fg(theme.muted))
            .block(
                Block::default()
                    .borders(Borders::TOP)
                    .border_style(Style::default().fg(theme.border)),
            );
        frame.render_widget(msg, area);
    }
}

/// Map a health score to a human-readable status label.
fn health_status_label(score: u8) -> &'static str {
    if score >= 80 {
        "Healthy"
    } else if score >= 50 {
        "Degraded"
    } else {
        "Critical"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::app::{App, DataSnapshot};
    use crate::tui::theme::Theme;
    use pulsos_core::config::types::TuiConfig;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    fn test_app_with_health() -> App {
        let mut data = DataSnapshot::default();
        data.health_scores = vec![
            ("my-saas".into(), 95),
            ("api-server".into(), 65),
            ("frontend".into(), 30),
        ];
        data.health_history = vec![
            ("my-saas".into(), vec![80, 85, 90, 92, 95]),
            ("api-server".into(), vec![70, 60, 55, 60, 65]),
        ];
        let mut config = TuiConfig::default();
        config.show_sparklines = true;
        App::new(data, config)
    }

    #[test]
    fn health_tab_renders_without_panic() {
        let backend = TestBackend::new(80, 20);
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
        assert!(text.contains("Healthy"), "Should show status label");
    }

    #[test]
    fn health_tab_renders_empty() {
        let backend = TestBackend::new(80, 10);
        let mut terminal = Terminal::new(backend).unwrap();

        let app = App::new(DataSnapshot::default(), TuiConfig::default());
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
        assert_eq!(health_status_label(80), "Healthy");
        assert_eq!(health_status_label(79), "Degraded");
        assert_eq!(health_status_label(50), "Degraded");
        assert_eq!(health_status_label(49), "Critical");
        assert_eq!(health_status_label(0), "Critical");
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
