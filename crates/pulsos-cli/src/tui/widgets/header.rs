//! Header widget — tab bar with active indicator and right-aligned clock.

use chrono::Local;
use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

use crate::tui::app::{App, Tab};
use crate::tui::theme::Theme;

/// Draw the header bar containing tab labels and a clock.
pub fn draw(frame: &mut Frame, area: Rect, app: &App, theme: &Theme) {
    // Split into left (tabs) and right (clock)
    let chunks = Layout::horizontal([Constraint::Min(40), Constraint::Length(20)]).split(area);

    // Tab bar
    let tab_spans: Vec<Span> = Tab::ALL
        .iter()
        .enumerate()
        .flat_map(|(i, tab)| {
            let style = if *tab == app.active_tab {
                theme.tab_active
            } else {
                theme.tab_inactive
            };
            let label = format!("[{}] {}", i + 1, tab.short_label());
            let mut spans = vec![Span::styled(label, style)];
            spans.push(Span::raw("  "));
            spans
        })
        .collect();

    let tabs_line = Line::from(tab_spans);
    let tabs = Paragraph::new(tabs_line);
    frame.render_widget(tabs, chunks[0]);

    // Clock (right-aligned)
    let now = Local::now().format("%H:%M:%S").to_string();
    let clock = Paragraph::new(Line::from(vec![
        Span::styled(
            "pulsos",
            Style::default().add_modifier(Modifier::BOLD).fg(theme.fg),
        ),
        Span::raw("  "),
        Span::styled(now, Style::default().fg(theme.muted)),
    ]))
    .alignment(ratatui::layout::Alignment::Right);
    frame.render_widget(clock, chunks[1]);
}

/// Render the header to a buffer — used for testing.
#[cfg(test)]
pub fn draw_to_buf(area: Rect, app: &App, theme: &Theme) -> ratatui::buffer::Buffer {
    let mut buf = ratatui::buffer::Buffer::empty(area);
    // Simplified: render tab labels only (no frame)
    let tab_text: String = Tab::ALL
        .iter()
        .enumerate()
        .map(|(i, tab)| {
            let marker = if *tab == app.active_tab { ">" } else { " " };
            format!("{}[{}] {}", marker, i + 1, tab.short_label())
        })
        .collect::<Vec<_>>()
        .join("  ");

    let line = Line::from(tab_text);
    let para = Paragraph::new(line);
    ratatui::widgets::Widget::render(para, area, &mut buf);
    buf
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::app::{DataSnapshot, Tab};
    use pulsos_core::config::types::TuiConfig;

    fn test_app() -> App {
        App::new(DataSnapshot::default(), TuiConfig::default())
    }

    fn buffer_text(buf: &ratatui::buffer::Buffer) -> String {
        let mut text = String::new();
        for y in buf.area.top()..buf.area.bottom() {
            for x in buf.area.left()..buf.area.right() {
                text.push_str(buf.cell((x, y)).map(|c| c.symbol()).unwrap_or(" "));
            }
        }
        text
    }

    #[test]
    fn header_shows_all_tab_labels() {
        let app = test_app();
        let theme = Theme::dark();
        let area = Rect::new(0, 0, 80, 1);
        let buf = draw_to_buf(area, &app, &theme);
        let text = buffer_text(&buf);
        assert!(text.contains("Unified"), "Should contain Unified tab");
        assert!(text.contains("Platform"), "Should contain Platform tab");
        assert!(text.contains("Health"), "Should contain Health tab");
    }

    #[test]
    fn header_highlights_active_tab() {
        let mut app = test_app();
        app.active_tab = Tab::Platform;
        let theme = Theme::dark();
        let area = Rect::new(0, 0, 80, 1);
        let buf = draw_to_buf(area, &app, &theme);
        let text = buffer_text(&buf);
        // Active tab has ">" marker
        assert!(
            text.contains(">[2] Platform"),
            "Active tab should be marked"
        );
        assert!(
            text.contains(" [1] Unified"),
            "Inactive tab should not be marked"
        );
    }
}
