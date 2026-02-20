//! Header widget — 3-line brand mark + tab bar + active-tab underline.
//!
//! Layout (§4.17, §5.2):
//!   Line 1: `P U L S O S   Unified │ Platform │ Health    Last sync: Xs ago`
//!   Line 2:  underline `════` only under the active tab (accent.primary)
//!   Line 3:  (reserved / blank)

use chrono::Utc;
use pulsos_core::auth::PlatformKind;
use pulsos_core::health::PlatformHealthState;
use ratatui::{
    layout::{Alignment, Constraint, Layout, Rect},
    style::Style,
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

use crate::tui::app::{App, Tab};
use crate::tui::theme::Theme;

/// Draw the 3-row header.
pub fn draw(frame: &mut Frame, area: Rect, app: &App, theme: &Theme) {
    let rows = Layout::vertical([
        Constraint::Length(1), // line 1: brand + tabs + sync info
        Constraint::Length(1), // line 2: active-tab underline
        Constraint::Length(1), // line 3: blank / reserved
    ])
    .split(area);

    // ── Line 1: brand | tabs | sync ──────────────────────────────────────────
    let brand_width = 13u16; // "P U L S O S  " = 13 chars
    let sync_width = 20u16;

    let row1_chunks = Layout::horizontal([
        Constraint::Length(brand_width),
        Constraint::Min(0),
        Constraint::Length(sync_width),
    ])
    .split(rows[0]);

    // Brand mark
    frame.render_widget(
        Paragraph::new(Span::styled("P U L S O S", theme.t1())),
        row1_chunks[0],
    );

    // Tabs with │ separators
    let mut tab_spans = build_tab_spans(app, theme);
    tab_spans.extend(build_platform_badges(app, theme));
    frame.render_widget(Paragraph::new(Line::from(tab_spans)), row1_chunks[1]);

    // Sync info (right-aligned)
    let sync_text = format_sync_status(app);
    frame.render_widget(
        Paragraph::new(Span::styled(sync_text, theme.t8())).alignment(Alignment::Right),
        row1_chunks[2],
    );

    // ── Line 2: active-tab underline ─────────────────────────────────────────
    let underline = build_underline(app, brand_width);
    frame.render_widget(
        Paragraph::new(Span::styled(
            underline,
            Style::new().fg(theme.accent_primary),
        )),
        rows[1],
    );
    // line 3 left blank
}

/// Build tab spans: `Unified │ Platform │ Health` with active tab highlighted.
fn build_tab_spans<'a>(app: &App, theme: &'a Theme) -> Vec<Span<'a>> {
    let mut spans = Vec::new();
    for (i, tab) in Tab::ALL.iter().enumerate() {
        if i > 0 {
            spans.push(Span::styled(" │ ", theme.t8()));
        }
        let style = if *tab == app.active_tab {
            theme.tab_active()
        } else {
            theme.tab_inactive()
        };
        spans.push(Span::styled(tab.short_label(), style));
    }
    spans
}

fn build_platform_badges<'a>(app: &App, theme: &'a Theme) -> Vec<Span<'a>> {
    let mut spans = Vec::new();
    if app.data.platform_health.is_empty() {
        return spans;
    }

    spans.push(Span::styled("   ", theme.t8()));
    for (idx, platform) in PlatformKind::ALL.iter().enumerate() {
        if idx > 0 {
            spans.push(Span::styled(" ", theme.t8()));
        }
        let state = app
            .data
            .platform_health
            .iter()
            .find(|report| report.platform == *platform)
            .map(|report| report.state)
            .unwrap_or(PlatformHealthState::NoToken);

        let style = match state {
            PlatformHealthState::Ready => theme.success(),
            PlatformHealthState::InvalidToken => theme.failure(),
            PlatformHealthState::ConnectivityError => theme.warning(),
            PlatformHealthState::AccessOrConfigIncomplete => theme.warning(),
            PlatformHealthState::NoToken => theme.neutral(),
        };

        spans.push(Span::styled(
            format!("{}{}", short_platform(platform), state.icon()),
            style,
        ));
    }

    spans
}

fn short_platform(platform: &PlatformKind) -> &'static str {
    match platform {
        PlatformKind::GitHub => "GH",
        PlatformKind::Railway => "RW",
        PlatformKind::Vercel => "VC",
    }
}

/// Build the underline string: spaces to align, then `═` under the active tab.
fn build_underline(app: &App, brand_prefix: u16) -> String {
    let tab_labels = ["Unified", "Platform", "Health", "Settings"];
    let active_idx = app.active_tab.index();

    // Offset = brand_prefix + widths of all preceding tabs + " │ " separators (3 chars each)
    let mut offset = brand_prefix as usize;
    for label in tab_labels.iter().take(active_idx) {
        offset += label.len() + 3; // " │ " = 3 chars
    }

    let width = tab_labels[active_idx].len();
    format!("{}{}", " ".repeat(offset), "═".repeat(width))
}

fn spinner_frame(app: &App) -> &'static str {
    const FRAMES: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
    let elapsed_ms = (Utc::now() - app.data.last_cycle_started_at)
        .num_milliseconds()
        .max(0);
    let idx = ((elapsed_ms / 80) as usize) % FRAMES.len();
    FRAMES[idx]
}

/// Format sync status for display in the header.
fn format_sync_status(app: &App) -> String {
    if app.data.is_syncing {
        return format!("{} syncing", spinner_frame(app));
    }

    let diff = Utc::now() - app.data.fetched_at;
    let secs = diff.num_seconds();
    if secs < 5 {
        "last: just now".into()
    } else if secs < 60 {
        format!("last: {secs}s ago")
    } else if secs < 3600 {
        format!("last: {}m ago", secs / 60)
    } else {
        format!("last: {}h ago", secs / 3600)
    }
}

/// Render the header to a buffer — used for testing.
#[cfg(test)]
pub fn draw_to_buf(area: Rect, app: &App, _theme: &Theme) -> ratatui::buffer::Buffer {
    let mut buf = ratatui::buffer::Buffer::empty(area);

    // Row 0: tab labels with │ separators
    let tab_text = Tab::ALL
        .iter()
        .enumerate()
        .map(|(i, tab)| {
            if i == 0 {
                tab.short_label().to_string()
            } else {
                format!(" │ {}", tab.short_label())
            }
        })
        .collect::<Vec<_>>()
        .join("");

    let para = Paragraph::new(Line::from(tab_text));
    let area_row0 = Rect::new(area.x, area.y, area.width, 1);
    ratatui::widgets::Widget::render(para, area_row0, &mut buf);

    // Row 1: underline under active tab (if area is tall enough)
    if area.height >= 2 {
        let tab_labels = ["Unified", "Platform", "Health", "Settings"];
        let active_idx = app.active_tab.index();
        let mut offset = 0usize;
        for label in tab_labels.iter().take(active_idx) {
            offset += label.len() + 3; // " │ " separator
        }
        let width = tab_labels[active_idx].len();
        let underline = format!("{}{}", " ".repeat(offset), "═".repeat(width));
        let underline_para = Paragraph::new(Line::from(underline));
        let area_row1 = Rect::new(area.x, area.y + 1, area.width, 1);
        ratatui::widgets::Widget::render(underline_para, area_row1, &mut buf);
    }

    buf
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::app::{DataSnapshot, Tab};
    use chrono::Duration;
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
        let area = Rect::new(0, 0, 80, 2);
        let buf = draw_to_buf(area, &app, &theme);
        let text = buffer_text(&buf);
        assert!(text.contains("Unified"), "Should contain Unified tab");
        assert!(text.contains("Platform"), "Should contain Platform tab");
        assert!(text.contains("Health"), "Should contain Health tab");
        assert!(text.contains("Settings"), "Should contain Settings tab");
    }

    #[test]
    fn header_highlights_active_tab() {
        let mut app = test_app();
        app.active_tab = Tab::Platform;
        let theme = Theme::dark();
        let area = Rect::new(0, 0, 80, 2);
        let buf = draw_to_buf(area, &app, &theme);
        let text = buffer_text(&buf);
        // Active tab name is present and underline character appears
        assert!(text.contains("Platform"), "Active tab should be visible");
        assert!(
            text.contains('═'),
            "Active tab should have underline character"
        );
    }

    #[test]
    fn header_underline_under_unified() {
        let app = test_app(); // default: Unified
        let theme = Theme::dark();
        let area = Rect::new(0, 0, 80, 2);
        let buf = draw_to_buf(area, &app, &theme);
        let text = buffer_text(&buf);
        assert!(text.contains('═'), "Unified tab should have underline");
    }

    #[test]
    fn sync_status_shows_spinner_when_syncing() {
        let mut app = test_app();
        app.data.is_syncing = true;
        app.data.last_cycle_started_at = Utc::now() - Duration::milliseconds(160);
        let text = format_sync_status(&app);
        assert!(text.contains("syncing"));
    }

    #[test]
    fn sync_status_shows_last_age_when_idle() {
        let mut app = test_app();
        app.data.is_syncing = false;
        app.data.fetched_at = Utc::now() - Duration::seconds(30);
        let text = format_sync_status(&app);
        assert!(text.starts_with("last: "));
    }
}
