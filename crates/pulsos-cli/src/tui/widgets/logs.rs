//! Tab 5: Logs — filter bar + scrollable table of captured tracing log entries.
//!
//! Layout: filter bar (1 row) + log table.
//! Level coloring: ERROR=failure(red), WARN=warning(yellow),
//! INFO=active(blue), DEBUG/TRACE=muted.

use ratatui::{
    layout::{Alignment, Constraint, Layout, Rect},
    text::{Line, Span},
    widgets::{Block, Borders, Cell, Paragraph, Row, Table, TableState},
    Frame,
};
use tracing::Level;

use crate::tui::app::{App, LogFilter};
use crate::tui::theme::Theme;
use crate::tui::widgets::{draw_search_bar, split_search_bar};

/// Draw the Logs tab.
pub fn draw(frame: &mut Frame, area: Rect, app: &App, theme: &Theme) {
    let (search_area, area) = split_search_bar(area, app);
    if let Some(sa) = search_area {
        draw_search_bar(frame, sa, app, theme);
    }

    let entries = app.log_buffer.snapshot();

    if entries.is_empty() {
        let msg = Paragraph::new("No log entries captured yet.").style(theme.t7());
        frame.render_widget(msg, area);
        return;
    }

    let chunks = Layout::vertical([Constraint::Length(1), Constraint::Min(3)]).split(area);

    // Row 0: Filter bar
    draw_filter_bar(frame, chunks[0], app, &entries, theme);

    // Row 1: Log table
    let filtered: Vec<_> = entries
        .iter()
        .filter(|e| app.log_filter.matches(&e.level))
        .collect();

    if filtered.is_empty() {
        let msg = Paragraph::new("No entries match the active filter.").style(theme.t8());
        frame.render_widget(msg, chunks[1]);
        return;
    }

    let header_cells = ["Time", "Level", "Target", "Message"]
        .iter()
        .map(|h| Cell::from(*h).style(theme.t4()));
    let header = Row::new(header_cells).height(1);

    let rows: Vec<Row> = filtered
        .iter()
        .map(|entry| {
            let time = entry.timestamp.format("%H:%M:%S").to_string();
            let (level_str, level_style) = level_badge(&entry.level, theme);
            let target = shorten_target(&entry.target);

            Row::new(vec![
                Cell::from(Span::styled(time, theme.t8())),
                Cell::from(Span::styled(level_str, level_style)),
                Cell::from(Span::styled(target, theme.t7())),
                Cell::from(Span::styled(entry.message.clone(), theme.t6())),
            ])
        })
        .collect();

    let widths = [
        Constraint::Length(10), // Time
        Constraint::Length(5),  // Level
        Constraint::Length(30), // Target
        Constraint::Min(20),    // Message
    ];

    let table = Table::new(rows, widths)
        .header(header)
        .block(Block::default().borders(Borders::NONE))
        .row_highlight_style(theme.selected_row())
        .highlight_symbol("▶ ");

    let mut table_state = TableState::default();
    if !filtered.is_empty() {
        table_state.select(Some(app.selected_row.min(filtered.len().saturating_sub(1))));
    }

    frame.render_stateful_widget(table, chunks[1], &mut table_state);
}

/// Draw the filter bar: `[ALL] [ERR] [WARN] [INFO]   {n} entries`
fn draw_filter_bar(
    frame: &mut Frame,
    area: Rect,
    app: &App,
    entries: &[crate::tui::log_buffer::LogEntry],
    theme: &Theme,
) {
    let bar_cols = Layout::horizontal([Constraint::Min(30), Constraint::Length(16)]).split(area);

    let filters = [
        LogFilter::All,
        LogFilter::Error,
        LogFilter::Warn,
        LogFilter::Info,
    ];
    let mut spans: Vec<Span> = Vec::new();

    for (i, filter) in filters.iter().enumerate() {
        if i > 0 {
            spans.push(Span::raw(" "));
        }
        let label = format!("[{}]", filter.label());
        if *filter == app.log_filter {
            let style = match filter {
                LogFilter::Error => theme.failure(),
                LogFilter::Warn => theme.warning(),
                LogFilter::Info => theme.active(),
                LogFilter::All => theme.keybind_key(),
            };
            spans.push(Span::styled(label, style));
        } else {
            spans.push(Span::styled(label, theme.t8()));
        }
    }

    frame.render_widget(Paragraph::new(Line::from(spans)), bar_cols[0]);

    let count = entries
        .iter()
        .filter(|e| app.log_filter.matches(&e.level))
        .count();
    let count_text = format!("{count} entries");
    let right = Paragraph::new(Line::from(Span::styled(count_text, theme.t8())))
        .alignment(Alignment::Right);
    frame.render_widget(right, bar_cols[1]);
}

/// Shorten a tracing target path by stripping the `pulsos_core::` or `pulsos_cli::` prefix.
fn shorten_target(target: &str) -> String {
    for prefix in &["pulsos_core::", "pulsos_cli::"] {
        if let Some(rest) = target.strip_prefix(prefix) {
            return rest.to_string();
        }
    }
    target.to_string()
}

fn level_badge(level: &Level, theme: &Theme) -> (String, ratatui::style::Style) {
    match *level {
        Level::ERROR => ("ERROR".into(), theme.failure()),
        Level::WARN => (" WARN".into(), theme.warning()),
        Level::INFO => (" INFO".into(), theme.active()),
        Level::DEBUG => ("DEBUG".into(), theme.neutral()),
        Level::TRACE => ("TRACE".into(), theme.neutral()),
    }
}

/// Short level abbreviation for footer display.
pub fn level_abbrev(level: &Level) -> &'static str {
    match *level {
        Level::ERROR => "ERR",
        Level::WARN => "WRN",
        Level::INFO => "INF",
        Level::DEBUG => "DBG",
        Level::TRACE => "TRC",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::app::{App, DataSnapshot};
    use crate::tui::log_buffer::{LogEntry, LogRingBuffer};
    use chrono::Utc;
    use pulsos_core::config::types::TuiConfig;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    fn test_app_with_logs() -> App {
        let log_buffer = LogRingBuffer::new();
        log_buffer.push(LogEntry {
            timestamp: Utc::now(),
            level: Level::WARN,
            target: "pulsos_core::platform::github".into(),
            message: "retry attempt 1 for GitHub".into(),
        });
        log_buffer.push(LogEntry {
            timestamp: Utc::now(),
            level: Level::INFO,
            target: "pulsos_core::platform::vercel".into(),
            message: "cache hit for vercel-project".into(),
        });
        log_buffer.push(LogEntry {
            timestamp: Utc::now(),
            level: Level::ERROR,
            target: "pulsos_core::platform::railway".into(),
            message: "connection refused".into(),
        });
        let mut app = App::new(DataSnapshot::default(), TuiConfig::default(), log_buffer);
        app.active_tab = crate::tui::app::Tab::Logs;
        app.selected_row = 2; // last entry (auto-scroll position)
        app
    }

    #[test]
    fn logs_tab_renders_without_panic() {
        let backend = TestBackend::new(80, 20);
        let mut terminal = Terminal::new(backend).unwrap();

        let app = test_app_with_logs();
        let theme = Theme::dark();

        terminal
            .draw(|frame| {
                draw(frame, frame.area(), &app, &theme);
            })
            .unwrap();

        let buf = terminal.backend().buffer().clone();
        let text = buffer_to_string(&buf);
        assert!(text.contains("retry attempt"), "Should show log message");
        assert!(text.contains("WARN"), "Should show level badge");
        assert!(text.contains("[ALL]"), "Should show filter bar");
    }

    #[test]
    fn logs_tab_renders_empty() {
        let backend = TestBackend::new(80, 10);
        let mut terminal = Terminal::new(backend).unwrap();

        let log_buffer = LogRingBuffer::new();
        let mut app = App::new(DataSnapshot::default(), TuiConfig::default(), log_buffer);
        app.active_tab = crate::tui::app::Tab::Logs;
        let theme = Theme::dark();

        terminal
            .draw(|frame| {
                draw(frame, frame.area(), &app, &theme);
            })
            .unwrap();

        let buf = terminal.backend().buffer().clone();
        let text = buffer_to_string(&buf);
        assert!(text.contains("No log entries"), "Should show empty message");
    }

    #[test]
    fn level_abbrev_values() {
        assert_eq!(level_abbrev(&Level::ERROR), "ERR");
        assert_eq!(level_abbrev(&Level::WARN), "WRN");
        assert_eq!(level_abbrev(&Level::INFO), "INF");
        assert_eq!(level_abbrev(&Level::DEBUG), "DBG");
        assert_eq!(level_abbrev(&Level::TRACE), "TRC");
    }

    #[test]
    fn logs_filter_bar_with_active_filter() {
        let backend = TestBackend::new(80, 20);
        let mut terminal = Terminal::new(backend).unwrap();

        let mut app = test_app_with_logs();
        app.log_filter = LogFilter::Error;
        app.selected_row = 0;
        let theme = Theme::dark();

        terminal
            .draw(|frame| {
                draw(frame, frame.area(), &app, &theme);
            })
            .unwrap();

        let buf = terminal.backend().buffer().clone();
        let text = buffer_to_string(&buf);
        assert!(text.contains("[ERR]"), "Should show ERR filter");
        assert!(
            text.contains("connection refused"),
            "Should show ERROR entries"
        );
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
