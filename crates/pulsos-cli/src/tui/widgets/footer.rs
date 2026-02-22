//! Footer widget — `[key] desc` keybinding badges + refresh age + warnings.

use chrono::Utc;
use ratatui::{
    layout::{Constraint, Layout, Rect},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

use crate::tui::app::{App, InputMode, Tab};
use crate::tui::settings_flow::SettingsFlowState;
use crate::tui::theme::Theme;

mod copy {
    pub const SETTINGS_IDLE: &str =
        "[Enter] onboard  [t/T] token  [v] validate  [x] remove  [r] refresh";
    pub const SETTINGS_PROVIDER_SELECT: &str =
        "[↑↓] move  [Space] toggle  [Enter] discover  [Esc] cancel";
    pub const SETTINGS_RESOURCE_SELECT: &str =
        "[↑↓] move  [Space] toggle  [Enter] preview  [Esc] cancel";
    pub const SETTINGS_PREVIEW: &str = "[Enter] apply  [Esc] back";
    pub const SETTINGS_BUSY: &str = "[... ] working";
    pub const SETTINGS_TOKEN_INPUT: &str = "[Enter] validate+save  [Esc] cancel";
}

/// Draw the footer bar (2 rows: keybindings + sync status on top, log/warning below).
pub fn draw(frame: &mut Frame, area: Rect, app: &App, theme: &Theme) {
    let rows = Layout::vertical([Constraint::Length(1), Constraint::Length(1)]).split(area);

    // ── Row 0: keybinding help (left) + sync status (right) ──
    let top = Layout::horizontal([Constraint::Min(36), Constraint::Length(20)]).split(rows[0]);

    let help_line = match app.input_mode {
        InputMode::Normal => build_normal_help(app, theme),
        InputMode::Search => build_search_help(app, theme),
    };
    frame.render_widget(Paragraph::new(help_line), top[0]);

    let status = format_sync_status(app);
    let right = Paragraph::new(Line::from(Span::styled(status, theme.keybind_desc())))
        .alignment(ratatui::layout::Alignment::Right);
    frame.render_widget(right, top[1]);

    // ── Row 1: warning or latest log entry ──
    let max_msg = (rows[1].width as usize).saturating_sub(6); // room for "WRN "
    let warning_count = app.data.warnings.len();
    let bottom_spans: Vec<Span> = if warning_count > 0 {
        let mut spans = vec![Span::styled(format!("⚠ {warning_count}"), theme.warning())];
        if let Some(last) = app.data.warnings.last() {
            spans.push(Span::raw("  "));
            spans.push(Span::styled(truncate(last, max_msg), theme.t8()));
        }
        spans
    } else if let Some(entry) = app.log_buffer.latest() {
        let abbrev = super::logs::level_abbrev(&entry.level);
        vec![Span::styled(
            format!("{abbrev} {}", truncate(&entry.message, max_msg)),
            theme.t8(),
        )]
    } else {
        vec![]
    };

    if !bottom_spans.is_empty() {
        frame.render_widget(Paragraph::new(Line::from(bottom_spans)), rows[1]);
    }
}

/// Normal-mode keybinding line: `[key] desc` pairs.
fn build_normal_help(app: &App, theme: &Theme) -> Line<'static> {
    if app.active_tab == Tab::Settings {
        return Line::from(Span::styled(
            settings_help_text(app.settings_flow).to_string(),
            theme.keybind_desc(),
        ));
    }

    let entries: Vec<(&str, &str)> = vec![
        ("[q]", "quit"),
        ("[Tab]", "switch tab (1-5)"),
        ("[↵]", "select"),
        ("[/]", "search"),
        ("[r]", "refresh"),
    ];
    let mut spans: Vec<Span<'static>> = Vec::new();
    for (i, (key, desc)) in entries.iter().enumerate() {
        if i > 0 {
            spans.push(Span::styled("  ", theme.keybind_desc()));
        }
        spans.push(Span::styled(*key, theme.keybind_key()));
        spans.push(Span::styled(" ", theme.keybind_desc()));
        spans.push(Span::styled(*desc, theme.keybind_desc()));
    }
    if app.active_tab == Tab::Logs {
        spans.push(Span::styled("  ", theme.keybind_desc()));
        spans.push(Span::styled("[f]", theme.keybind_key()));
        spans.push(Span::styled(" ", theme.keybind_desc()));
        spans.push(Span::styled("filter", theme.keybind_desc()));
    }
    if app.active_tab == Tab::Unified {
        spans.push(Span::styled("  ", theme.keybind_desc()));
        spans.push(Span::styled("[s]", theme.keybind_key()));
        spans.push(Span::styled(" ", theme.keybind_desc()));
        spans.push(Span::styled(
            format!("sort: {}", app.unified_sort.label()),
            theme.keybind_desc(),
        ));
    }
    Line::from(spans)
}

/// Search-mode keybinding line: `[Esc] cancel  [↵] apply  Filter: {query}█`
fn build_search_help<'a>(app: &'a App, theme: &'a Theme) -> Line<'a> {
    let entries: &[(&str, &str)] = &[("[Esc]", "cancel"), ("[↵]", "apply")];

    let mut spans: Vec<Span<'a>> = Vec::new();
    for (i, (key, desc)) in entries.iter().enumerate() {
        if i > 0 {
            spans.push(Span::styled("  ", theme.keybind_desc()));
        }
        spans.push(Span::styled(*key, theme.keybind_key()));
        spans.push(Span::styled(" ", theme.keybind_desc()));
        spans.push(Span::styled(*desc, theme.keybind_desc()));
    }
    spans.push(Span::styled("  Filter: ", theme.keybind_desc()));
    spans.push(Span::styled(app.search_query.as_str(), theme.t5()));
    spans.push(Span::styled("█", theme.keybind_key()));
    Line::from(spans)
}

fn spinner_frame(app: &App) -> &'static str {
    const FRAMES: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
    let elapsed_ms = (Utc::now() - app.data.last_cycle_started_at)
        .num_milliseconds()
        .max(0);
    let idx = ((elapsed_ms / 80) as usize) % FRAMES.len();
    FRAMES[idx]
}

fn format_sync_status(app: &App) -> String {
    if app.data.is_syncing {
        return format!("{} syncing", spinner_frame(app));
    }

    let diff = Utc::now() - app.data.last_cycle_completed_at;
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

fn truncate(s: &str, max: usize) -> String {
    if max == 0 {
        return String::new();
    }
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let mut out: String = s.chars().take(max.saturating_sub(1)).collect();
        out.push('…');
        out
    }
}

fn settings_help_text(flow: SettingsFlowState) -> &'static str {
    match flow {
        SettingsFlowState::ProviderActions => copy::SETTINGS_PROVIDER_SELECT,
        SettingsFlowState::ResourceSelection => copy::SETTINGS_RESOURCE_SELECT,
        SettingsFlowState::CorrelationReview => copy::SETTINGS_PREVIEW,
        SettingsFlowState::ValidatingToken
        | SettingsFlowState::DiscoveryScanning
        | SettingsFlowState::Applying => copy::SETTINGS_BUSY,
        SettingsFlowState::TokenEntry => copy::SETTINGS_TOKEN_INPUT,
        SettingsFlowState::Idle | SettingsFlowState::ValidationResult => copy::SETTINGS_IDLE,
    }
}

/// Render footer help text as a plain string — used for testing.
#[cfg(test)]
pub fn render_help_text(app: &App) -> String {
    match app.input_mode {
        InputMode::Normal if app.active_tab == Tab::Settings => {
            settings_help_text(app.settings_flow).to_string()
        }
        InputMode::Normal if app.active_tab == Tab::Logs => {
            "[q] quit  [Tab] switch tab (1-5)  [↵] select  [/] search  [r] refresh  [f] filter"
                .to_string()
        }
        InputMode::Normal if app.active_tab == Tab::Unified => {
            format!(
                "[q] quit  [Tab] switch tab (1-5)  [↵] select  [/] search  [r] refresh  [s] sort: {}",
                app.unified_sort.label()
            )
        }
        InputMode::Normal => {
            "[q] quit  [Tab] switch tab (1-5)  [↵] select  [/] search  [r] refresh".to_string()
        }
        InputMode::Search => format!("[Esc] cancel  [↵] apply  Filter: {}█", app.search_query),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::app::{DataSnapshot, InputMode};
    use crate::tui::log_buffer::LogRingBuffer;
    use pulsos_core::config::types::TuiConfig;

    fn test_app() -> App {
        App::new(
            DataSnapshot::default(),
            TuiConfig::default(),
            LogRingBuffer::new(),
        )
    }

    #[test]
    fn footer_shows_normal_mode_help() {
        let app = test_app();
        let text = render_help_text(&app);
        assert!(text.contains("[q]"));
        assert!(text.contains("quit"));
        assert!(text.contains("[/]"));
        assert!(text.contains("search"));
    }

    #[test]
    fn footer_shows_search_mode_help() {
        let mut app = test_app();
        app.input_mode = InputMode::Search;
        app.search_query = "prod".into();
        let text = render_help_text(&app);
        assert!(text.contains("Filter: prod"));
        assert!(text.contains("cancel"));
    }

    #[test]
    fn settings_idle_result_legend_is_terse() {
        let mut app = test_app();
        app.active_tab = Tab::Settings;
        app.settings_flow = SettingsFlowState::Idle;
        assert_eq!(render_help_text(&app), copy::SETTINGS_IDLE);

        app.settings_flow = SettingsFlowState::ValidationResult;
        assert_eq!(render_help_text(&app), copy::SETTINGS_IDLE);
    }

    #[test]
    fn settings_provider_select_legend() {
        let mut app = test_app();
        app.active_tab = Tab::Settings;
        app.settings_flow = SettingsFlowState::ProviderActions;
        assert_eq!(render_help_text(&app), copy::SETTINGS_PROVIDER_SELECT);
    }

    #[test]
    fn settings_resource_select_legend() {
        let mut app = test_app();
        app.active_tab = Tab::Settings;
        app.settings_flow = SettingsFlowState::ResourceSelection;
        assert_eq!(render_help_text(&app), copy::SETTINGS_RESOURCE_SELECT);
    }

    #[test]
    fn settings_preview_legend() {
        let mut app = test_app();
        app.active_tab = Tab::Settings;
        app.settings_flow = SettingsFlowState::CorrelationReview;
        assert_eq!(render_help_text(&app), copy::SETTINGS_PREVIEW);
    }

    #[test]
    fn settings_busy_legend() {
        let mut app = test_app();
        app.active_tab = Tab::Settings;
        app.settings_flow = SettingsFlowState::Applying;
        assert_eq!(render_help_text(&app), copy::SETTINGS_BUSY);
    }

    #[test]
    fn footer_shows_filter_hint_on_logs_tab() {
        let mut app = test_app();
        app.active_tab = Tab::Logs;
        let text = render_help_text(&app);
        assert!(text.contains("[f]"));
        assert!(text.contains("filter"));
    }

    #[test]
    fn footer_no_filter_hint_on_other_tabs() {
        let mut app = test_app();
        app.active_tab = Tab::Platform; // non-Unified, non-Logs
        let text = render_help_text(&app);
        assert!(!text.contains("[f]"));
    }

    #[test]
    fn footer_shows_sort_hint_on_unified_tab() {
        let mut app = test_app();
        app.active_tab = Tab::Unified;
        let text = render_help_text(&app);
        assert!(text.contains("[s]"), "should contain sort key");
        assert!(text.contains("sort: time"), "should show default sort mode");
    }

    #[test]
    fn footer_sort_hint_updates_when_sort_changes() {
        use crate::tui::app::UnifiedSort;
        let mut app = test_app();
        app.active_tab = Tab::Unified;
        app.unified_sort = UnifiedSort::ByPlatform;
        let text = render_help_text(&app);
        assert!(text.contains("sort: platform"));
    }

    #[test]
    fn refresh_age_just_now() {
        let app = test_app();
        let age = format_sync_status(&app);
        assert_eq!(age, "just now");
    }

    #[test]
    fn refresh_age_old_data() {
        let mut app = test_app();
        let old = Utc::now() - chrono::Duration::seconds(120);
        app.data.fetched_at = old;
        app.data.last_cycle_completed_at = old;
        let age = format_sync_status(&app);
        assert_eq!(age, "2m ago");
    }

    #[test]
    fn sync_status_shows_spinner_when_syncing() {
        let mut app = test_app();
        app.data.is_syncing = true;
        let status = format_sync_status(&app);
        assert!(status.contains("syncing"));
    }

    #[test]
    fn truncate_latest_warning_summary() {
        let msg = truncate("this is a very long warning message", 10);
        assert_eq!(msg.chars().count(), 10);
        assert!(msg.ends_with('…'));
    }
}
