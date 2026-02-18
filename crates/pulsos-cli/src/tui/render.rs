//! Master render function — splits the terminal into header, main content, and footer.

use ratatui::{
    layout::{Constraint, Layout, Rect},
    Frame,
};

use super::app::{App, Tab};
use super::theme::Theme;
use super::widgets::{footer, header};

/// Render the entire TUI dashboard.
pub fn draw(frame: &mut Frame, app: &App, theme: &Theme) {
    let area = frame.area();

    // Check minimum terminal size
    if area.width < 60 || area.height < 10 {
        let msg = ratatui::widgets::Paragraph::new("Terminal too small. Resize to at least 60×10.")
            .style(ratatui::style::Style::default().fg(theme.failure));
        frame.render_widget(msg, area);
        return;
    }

    // Split: header (1 line) + main (flex) + footer (1 line)
    let chunks = Layout::vertical([
        Constraint::Length(1), // Header
        Constraint::Min(5),    // Main content
        Constraint::Length(1), // Footer
    ])
    .split(area);

    // Header
    header::draw(frame, chunks[0], app, theme);

    // Main content — delegate to active tab
    draw_tab_content(frame, chunks[1], app, theme);

    // Footer
    footer::draw(frame, chunks[2], app, theme);
}

fn draw_tab_content(frame: &mut Frame, area: Rect, app: &App, theme: &Theme) {
    match app.active_tab {
        Tab::Unified => super::widgets::unified::draw(frame, area, app, theme),
        Tab::Platform => super::widgets::platform::draw(frame, area, app, theme),
        Tab::Health => super::widgets::health::draw(frame, area, app, theme),
    }
}
