//! Master render function — splits the terminal into header, main content, and footer.

use ratatui::{
    layout::{Constraint, Layout, Rect},
    text::Line,
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
    Frame,
};

use super::app::{App, Tab};
use super::settings_flow::SettingsFlowState;
use super::theme::Theme;
use super::widgets::{footer, header};

/// Render the entire TUI dashboard.
pub fn draw(frame: &mut Frame, app: &App, theme: &Theme) {
    let area = frame.area();

    // Check minimum terminal size
    if area.width < 60 || area.height < 10 {
        let msg = ratatui::widgets::Paragraph::new("Terminal too small. Resize to at least 60×10.")
            .style(ratatui::style::Style::default().fg(theme.status_failure));
        frame.render_widget(msg, area);
        return;
    }

    // Split: header (2 lines) + main (flex) + footer (1 line)
    let chunks = Layout::vertical([
        Constraint::Length(2), // Header: brand + tabs + underline
        Constraint::Min(5),    // Main content
        Constraint::Length(2), // Footer: key hints + log/warning line
    ])
    .split(area);

    // Header
    header::draw(frame, chunks[0], app, theme);

    // Main content — delegate to active tab
    draw_tab_content(frame, chunks[1], app, theme);

    // Footer
    footer::draw(frame, chunks[2], app, theme);

    // Overlay modal (Settings token entry).
    if app.active_tab == Tab::Settings && app.settings_flow == SettingsFlowState::TokenEntry {
        draw_token_modal(frame, area, app, theme);
    }
}

fn draw_tab_content(frame: &mut Frame, area: Rect, app: &App, theme: &Theme) {
    match app.active_tab {
        Tab::Unified => super::widgets::unified::draw(frame, area, app, theme),
        Tab::Platform => super::widgets::platform::draw(frame, area, app, theme),
        Tab::Health => super::widgets::health::draw(frame, area, app, theme),
        Tab::Settings => super::widgets::settings::draw(frame, area, app, theme),
        Tab::Logs => super::widgets::logs::draw(frame, area, app, theme),
    }
}

fn draw_token_modal(frame: &mut Frame, area: Rect, app: &App, theme: &Theme) {
    let popup = centered_rect(60, 25, area);
    frame.render_widget(Clear, popup);

    let masked = "•".repeat(app.token_input.chars().count().min(80));
    let content = vec![
        Line::from("Token Input"),
        Line::from("enter token, Enter to validate+save"),
        Line::from(""),
        Line::from(format!("  {masked}")),
        Line::from(""),
        Line::from("Esc cancel"),
    ];

    let widget = Paragraph::new(content)
        .style(theme.t6())
        .wrap(Wrap { trim: false })
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(theme.panel_border_focus())
                .title(" Token "),
        );

    frame.render_widget(widget, popup);
}

fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let vertical = Layout::vertical([
        Constraint::Percentage((100 - percent_y) / 2),
        Constraint::Percentage(percent_y),
        Constraint::Percentage((100 - percent_y) / 2),
    ])
    .split(r);

    Layout::horizontal([
        Constraint::Percentage((100 - percent_x) / 2),
        Constraint::Percentage(percent_x),
        Constraint::Percentage((100 - percent_x) / 2),
    ])
    .split(vertical[1])[1]
}
