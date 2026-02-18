//! TUI application state — the single source of truth for the UI.

use chrono::{DateTime, Utc};
use pulsos_core::config::types::TuiConfig;
use pulsos_core::domain::deployment::DeploymentEvent;
use pulsos_core::domain::project::CorrelatedEvent;

/// The three top-level tabs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Unified,
    Platform,
    Health,
}

impl Tab {
    pub const ALL: [Tab; 3] = [Tab::Unified, Tab::Platform, Tab::Health];

    pub fn index(self) -> usize {
        match self {
            Tab::Unified => 0,
            Tab::Platform => 1,
            Tab::Health => 2,
        }
    }

    pub fn from_index(i: usize) -> Self {
        match i % 3 {
            0 => Tab::Unified,
            1 => Tab::Platform,
            2 => Tab::Health,
            _ => unreachable!(),
        }
    }

    #[allow(dead_code)]
    pub fn label(self) -> &'static str {
        match self {
            Tab::Unified => "Unified Overview",
            Tab::Platform => "Platform Details",
            Tab::Health => "Health & Metrics",
        }
    }

    pub fn short_label(self) -> &'static str {
        match self {
            Tab::Unified => "Unified",
            Tab::Platform => "Platform",
            Tab::Health => "Health",
        }
    }

    pub fn next(self) -> Self {
        Self::from_index(self.index() + 1)
    }

    pub fn prev(self) -> Self {
        Self::from_index((self.index() + 2) % 3)
    }
}

/// Whether the user is typing a search query or navigating normally.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputMode {
    Normal,
    Search,
}

/// A snapshot of all data needed to render the TUI.
///
/// Produced by the background poller and consumed by the renderer.
/// Intentionally `Clone` — data volumes are small (tens of events).
#[derive(Debug, Clone)]
pub struct DataSnapshot {
    /// Raw events from all platforms.
    pub events: Vec<DeploymentEvent>,
    /// Events correlated across platforms by commit SHA.
    pub correlated: Vec<CorrelatedEvent>,
    /// Per-project health scores (name, 0-100).
    pub health_scores: Vec<(String, u8)>,
    /// Per-project score history for sparklines (name, last N scores).
    pub health_history: Vec<(String, Vec<u8>)>,
    /// Warnings from platform fetches.
    pub warnings: Vec<String>,
    /// When the snapshot was created.
    pub fetched_at: DateTime<Utc>,
}

impl Default for DataSnapshot {
    fn default() -> Self {
        Self {
            events: Vec::new(),
            correlated: Vec::new(),
            health_scores: Vec::new(),
            health_history: Vec::new(),
            warnings: Vec::new(),
            fetched_at: Utc::now(),
        }
    }
}

/// The complete TUI application state.
#[allow(dead_code)]
pub struct App {
    /// Which tab is active.
    pub active_tab: Tab,
    /// Normal navigation or search input.
    pub input_mode: InputMode,
    /// Currently selected row index in the active tab's table.
    pub selected_row: usize,
    /// Vertical scroll offset for long tables.
    pub scroll_offset: usize,
    /// Current search/filter query.
    pub search_query: String,
    /// The latest data snapshot from the poller.
    pub data: DataSnapshot,
    /// TUI configuration (fps, theme, refresh interval, etc.).
    pub tui_config: TuiConfig,
    /// Set to `true` to exit the main loop.
    pub should_quit: bool,
    /// Set to `true` to bypass throttle on next poll cycle.
    pub force_refresh: bool,
    /// Current terminal dimensions (width, height).
    pub terminal_size: (u16, u16),
    /// Last error message for status bar display.
    pub last_error: Option<String>,
}

impl App {
    pub fn new(data: DataSnapshot, tui_config: TuiConfig) -> Self {
        let default_tab = match tui_config.default_tab.as_str() {
            "by_platform" | "platform" => Tab::Platform,
            "health" => Tab::Health,
            _ => Tab::Unified,
        };

        Self {
            active_tab: default_tab,
            input_mode: InputMode::Normal,
            selected_row: 0,
            scroll_offset: 0,
            search_query: String::new(),
            data,
            tui_config,
            should_quit: false,
            force_refresh: false,
            terminal_size: (80, 24),
            last_error: None,
        }
    }

    /// Number of displayable rows in the current tab.
    pub fn row_count(&self) -> usize {
        match self.active_tab {
            Tab::Unified => self.data.correlated.len(),
            Tab::Platform => self.data.events.len(),
            Tab::Health => self.data.health_scores.len(),
        }
    }

    /// Clamp `selected_row` to valid range after data or tab change.
    pub fn clamp_selection(&mut self) {
        let count = self.row_count();
        if count == 0 {
            self.selected_row = 0;
        } else if self.selected_row >= count {
            self.selected_row = count - 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tab_from_index_wraps() {
        assert_eq!(Tab::from_index(0), Tab::Unified);
        assert_eq!(Tab::from_index(1), Tab::Platform);
        assert_eq!(Tab::from_index(2), Tab::Health);
        assert_eq!(Tab::from_index(3), Tab::Unified);
        assert_eq!(Tab::from_index(4), Tab::Platform);
    }

    #[test]
    fn tab_next_prev_round_trip() {
        let tab = Tab::Unified;
        assert_eq!(tab.next(), Tab::Platform);
        assert_eq!(tab.next().next(), Tab::Health);
        assert_eq!(tab.next().next().next(), Tab::Unified);

        assert_eq!(tab.prev(), Tab::Health);
        assert_eq!(tab.prev().prev(), Tab::Platform);
        assert_eq!(tab.prev().prev().prev(), Tab::Unified);
    }

    #[test]
    fn tab_labels() {
        assert_eq!(Tab::Unified.label(), "Unified Overview");
        assert_eq!(Tab::Platform.label(), "Platform Details");
        assert_eq!(Tab::Health.label(), "Health & Metrics");
    }

    #[test]
    fn tab_short_labels() {
        assert_eq!(Tab::Unified.short_label(), "Unified");
        assert_eq!(Tab::Platform.short_label(), "Platform");
        assert_eq!(Tab::Health.short_label(), "Health");
    }

    #[test]
    fn app_default_tab_from_config() {
        let data = DataSnapshot::default();

        let config = TuiConfig::default();
        let app = App::new(data.clone(), config);
        assert_eq!(app.active_tab, Tab::Unified);

        let mut config = TuiConfig::default();
        config.default_tab = "platform".into();
        let app = App::new(data.clone(), config);
        assert_eq!(app.active_tab, Tab::Platform);

        let mut config = TuiConfig::default();
        config.default_tab = "health".into();
        let app = App::new(data, config);
        assert_eq!(app.active_tab, Tab::Health);
    }

    #[test]
    fn app_clamp_selection() {
        let data = DataSnapshot::default();
        let mut app = App::new(data, TuiConfig::default());

        app.selected_row = 10;
        app.clamp_selection();
        assert_eq!(app.selected_row, 0); // empty data

        app.selected_row = 0;
        app.clamp_selection();
        assert_eq!(app.selected_row, 0);
    }

    #[test]
    fn data_snapshot_default() {
        let snap = DataSnapshot::default();
        assert!(snap.events.is_empty());
        assert!(snap.correlated.is_empty());
        assert!(snap.health_scores.is_empty());
        assert!(snap.warnings.is_empty());
    }
}
