//! Footer widget — keybinding help, last refresh age, and warning count.

use chrono::Utc;
use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::Style,
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

use crate::tui::app::{App, InputMode};
use crate::tui::theme::Theme;

/// Draw the footer bar.
pub fn draw(frame: &mut Frame, area: Rect, app: &App, theme: &Theme) {
    let chunks = Layout::horizontal([Constraint::Min(40), Constraint::Length(30)]).split(area);

    // Left: keybinding help
    let help_text = match app.input_mode {
        InputMode::Normal => "j/k:navigate  1-3:tab  q:quit  r:refresh  /:search".to_string(),
        InputMode::Search => {
            format!("Search: {}█  (Enter:apply  Esc:cancel)", app.search_query)
        }
    };
    let help = Paragraph::new(Line::from(Span::styled(
        help_text,
        Style::default().fg(theme.muted),
    )));
    frame.render_widget(help, chunks[0]);

    // Right: last refresh age + warning count
    let age = format_refresh_age(app);
    let mut right_spans = vec![Span::styled(age, Style::default().fg(theme.muted))];

    let warning_count = app.data.warnings.len();
    if warning_count > 0 {
        right_spans.push(Span::raw("  "));
        right_spans.push(Span::styled(
            format!("⚠ {warning_count}"),
            Style::default().fg(theme.warning),
        ));
    }

    let right =
        Paragraph::new(Line::from(right_spans)).alignment(ratatui::layout::Alignment::Right);
    frame.render_widget(right, chunks[1]);
}

fn format_refresh_age(app: &App) -> String {
    let diff = Utc::now() - app.data.fetched_at;
    let secs = diff.num_seconds();
    if secs < 5 {
        "just now".into()
    } else if secs < 60 {
        format!("{secs}s ago")
    } else if secs < 3600 {
        format!("{}m ago", secs / 60)
    } else {
        format!("{}h ago", secs / 3600)
    }
}

/// Render footer to a buffer for testing.
#[cfg(test)]
pub fn render_help_text(app: &App) -> String {
    match app.input_mode {
        InputMode::Normal => "j/k:navigate  1-3:tab  q:quit  r:refresh  /:search".to_string(),
        InputMode::Search => {
            format!("Search: {}  (Enter:apply  Esc:cancel)", app.search_query)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::app::{DataSnapshot, InputMode};
    use pulsos_core::config::types::TuiConfig;

    fn test_app() -> App {
        App::new(DataSnapshot::default(), TuiConfig::default())
    }

    #[test]
    fn footer_shows_normal_mode_help() {
        let app = test_app();
        let text = render_help_text(&app);
        assert!(text.contains("j/k:navigate"));
        assert!(text.contains("q:quit"));
        assert!(text.contains("/:search"));
    }

    #[test]
    fn footer_shows_search_mode_help() {
        let mut app = test_app();
        app.input_mode = InputMode::Search;
        app.search_query = "prod".into();
        let text = render_help_text(&app);
        assert!(text.contains("Search: prod"));
        assert!(text.contains("Esc:cancel"));
    }

    #[test]
    fn refresh_age_just_now() {
        let app = test_app();
        let age = format_refresh_age(&app);
        assert_eq!(age, "just now");
    }

    #[test]
    fn refresh_age_old_data() {
        let mut app = test_app();
        app.data.fetched_at = Utc::now() - chrono::Duration::seconds(120);
        let age = format_refresh_age(&app);
        assert_eq!(age, "2m ago");
    }
}
