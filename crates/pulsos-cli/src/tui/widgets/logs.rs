//! Tab 5: Logs — scrollable list of captured tracing log entries.
//!
//! Each line: `HH:MM:SS  LEVEL  message`
//! Level coloring: ERROR=failure(red), WARN=warning(yellow),
//! INFO=active(blue), DEBUG/TRACE=muted.

use ratatui::{
    layout::Rect,
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};
use tracing::Level;

use crate::tui::app::App;
use crate::tui::theme::Theme;

/// Draw the Logs tab.
pub fn draw(frame: &mut Frame, area: Rect, app: &App, theme: &Theme) {
    let entries = app.log_buffer.snapshot();

    if entries.is_empty() {
        let msg =
            Paragraph::new("No log entries captured yet.").style(theme.t7());
        frame.render_widget(msg, area);
        return;
    }

    let visible_height = area.height as usize;
    let total = entries.len();

    // Auto-scroll: if selected_row is at or near the end, show the tail.
    // Otherwise respect the user's scroll position.
    let start = if app.selected_row >= total.saturating_sub(1) {
        // Auto-scroll to bottom
        total.saturating_sub(visible_height)
    } else if app.selected_row < app.scroll_offset {
        app.selected_row
    } else if app.selected_row >= app.scroll_offset + visible_height {
        app.selected_row.saturating_sub(visible_height - 1)
    } else {
        app.scroll_offset
    };

    let lines: Vec<Line> = entries
        .iter()
        .skip(start)
        .take(visible_height)
        .enumerate()
        .map(|(i, entry)| {
            let global_idx = start + i;
            let is_selected = global_idx == app.selected_row;

            let time = entry.timestamp.format("%H:%M:%S").to_string();
            let (level_str, level_style) = level_badge(&entry.level, theme);

            let mut spans = vec![
                Span::styled(time, theme.t8()),
                Span::raw("  "),
                Span::styled(level_str, level_style),
                Span::raw("  "),
            ];

            let msg_style = if is_selected {
                theme.selected_row().patch(theme.t6())
            } else {
                theme.t6()
            };
            spans.push(Span::styled(entry.message.clone(), msg_style));

            if is_selected {
                Line::from(spans).style(theme.selected_row())
            } else {
                Line::from(spans)
            }
        })
        .collect();

    let paragraph = Paragraph::new(lines).block(Block::default().borders(Borders::NONE));
    frame.render_widget(paragraph, area);
}

fn level_badge<'a>(level: &Level, theme: &'a Theme) -> (String, ratatui::style::Style) {
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
            message: "retry attempt 1 for GitHub".into(),
        });
        log_buffer.push(LogEntry {
            timestamp: Utc::now(),
            level: Level::INFO,
            message: "cache hit for vercel-project".into(),
        });
        log_buffer.push(LogEntry {
            timestamp: Utc::now(),
            level: Level::ERROR,
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
        assert!(
            text.contains("No log entries"),
            "Should show empty message"
        );
    }

    #[test]
    fn level_abbrev_values() {
        assert_eq!(level_abbrev(&Level::ERROR), "ERR");
        assert_eq!(level_abbrev(&Level::WARN), "WRN");
        assert_eq!(level_abbrev(&Level::INFO), "INF");
        assert_eq!(level_abbrev(&Level::DEBUG), "DBG");
        assert_eq!(level_abbrev(&Level::TRACE), "TRC");
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
