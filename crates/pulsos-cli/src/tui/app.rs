//! TUI application state — the single source of truth for the UI.

use chrono::{DateTime, Utc};
use pulsos_core::auth::PlatformKind;
use pulsos_core::config::types::PulsosConfig;
use pulsos_core::config::types::TuiConfig;
use pulsos_core::domain::deployment::DeploymentEvent;
use pulsos_core::domain::health::HealthBreakdown;
use pulsos_core::domain::project::CorrelatedEvent;
use pulsos_core::health::PlatformHealthReport;

use super::actions::{ActionRequest, ActionResult};
use super::log_buffer::LogRingBuffer;
use super::settings_flow::{OnboardingState, SettingsFlowState};

/// The top-level tabs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Unified,
    Platform,
    Health,
    Settings,
    Logs,
}

impl Tab {
    pub const ALL: [Tab; 5] = [
        Tab::Unified,
        Tab::Platform,
        Tab::Health,
        Tab::Settings,
        Tab::Logs,
    ];

    pub fn index(self) -> usize {
        match self {
            Tab::Unified => 0,
            Tab::Platform => 1,
            Tab::Health => 2,
            Tab::Settings => 3,
            Tab::Logs => 4,
        }
    }

    pub fn from_index(i: usize) -> Self {
        match i % 5 {
            0 => Tab::Unified,
            1 => Tab::Platform,
            2 => Tab::Health,
            3 => Tab::Settings,
            4 => Tab::Logs,
            _ => unreachable!(),
        }
    }

    #[allow(dead_code)]
    pub fn label(self) -> &'static str {
        match self {
            Tab::Unified => "Unified Overview",
            Tab::Platform => "Platform Details",
            Tab::Health => "Health & Metrics",
            Tab::Settings => "Settings",
            Tab::Logs => "Logs",
        }
    }

    pub fn short_label(self) -> &'static str {
        match self {
            Tab::Unified => "Unified",
            Tab::Platform => "Platform",
            Tab::Health => "Health",
            Tab::Settings => "Settings",
            Tab::Logs => "Logs",
        }
    }

    pub fn next(self) -> Self {
        Self::from_index(self.index() + 1)
    }

    pub fn prev(self) -> Self {
        Self::from_index((self.index() + Self::ALL.len() - 1) % Self::ALL.len())
    }
}

/// Whether the user is typing a search query or navigating normally.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputMode {
    Normal,
    Search,
}

/// Log level filter for the Logs tab.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogFilter {
    All,
    Error,
    Warn,
    Info,
}

impl LogFilter {
    pub fn next(self) -> Self {
        match self {
            Self::All => Self::Error,
            Self::Error => Self::Warn,
            Self::Warn => Self::Info,
            Self::Info => Self::All,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::All => "ALL",
            Self::Error => "ERR",
            Self::Warn => "WARN",
            Self::Info => "INFO",
        }
    }

    pub fn matches(self, level: &tracing::Level) -> bool {
        match self {
            Self::All => true,
            Self::Error => *level == tracing::Level::ERROR,
            Self::Warn => *level == tracing::Level::WARN,
            Self::Info => *level == tracing::Level::INFO,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct ActionOutcome {
    pub force_refresh: bool,
    pub replace_config: Option<PulsosConfig>,
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
    /// Per-project health breakdowns with per-platform scores and weights.
    pub health_breakdowns: Vec<(String, HealthBreakdown)>,
    /// Per-project score history for sparklines (name, last N scores).
    pub health_history: Vec<(String, Vec<u8>)>,
    /// Warnings from platform fetches.
    pub warnings: Vec<String>,
    /// Per-platform setup/auth/connectivity readiness reports.
    pub platform_health: Vec<PlatformHealthReport>,
    /// Whether a poll cycle is currently in flight.
    pub is_syncing: bool,
    /// When the snapshot was created.
    pub fetched_at: DateTime<Utc>,
    /// Poll cycle start time for live sync indicators.
    pub last_cycle_started_at: DateTime<Utc>,
    /// Poll cycle completion time for live sync indicators.
    pub last_cycle_completed_at: DateTime<Utc>,
}

impl Default for DataSnapshot {
    fn default() -> Self {
        let now = Utc::now();
        Self {
            events: Vec::new(),
            correlated: Vec::new(),
            health_scores: Vec::new(),
            health_breakdowns: Vec::new(),
            health_history: Vec::new(),
            warnings: Vec::new(),
            platform_health: Vec::new(),
            is_syncing: false,
            fetched_at: now,
            last_cycle_started_at: now,
            last_cycle_completed_at: now,
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
    /// Settings/Auth flow state.
    pub settings_flow: SettingsFlowState,
    /// Settings action selection cursor.
    pub settings_action_cursor: usize,
    /// Settings status message.
    pub settings_message: Option<String>,
    /// Token input buffer for masked entry modal.
    pub token_input: String,
    /// True when an async settings action is running.
    pub settings_action_in_flight: bool,
    /// Pending async action request to be sent by the main loop.
    pub pending_action: Option<ActionRequest>,
    /// In-TUI onboarding draft state.
    pub onboarding: OnboardingState,
    /// Captured tracing log entries for the Logs tab.
    pub log_buffer: LogRingBuffer,
    /// Active log level filter for the Logs tab.
    pub log_filter: LogFilter,
}

impl App {
    pub fn new(data: DataSnapshot, tui_config: TuiConfig, log_buffer: LogRingBuffer) -> Self {
        let default_tab = match tui_config.default_tab.as_str() {
            "by_platform" | "platform" => Tab::Platform,
            "health" => Tab::Health,
            "settings" => Tab::Settings,
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
            settings_flow: SettingsFlowState::Idle,
            settings_action_cursor: 0,
            settings_message: None,
            token_input: String::new(),
            settings_action_in_flight: false,
            pending_action: None,
            onboarding: OnboardingState::default(),
            log_buffer,
            log_filter: LogFilter::All,
        }
    }

    /// Number of displayable rows in the current tab.
    pub fn row_count(&self) -> usize {
        match self.active_tab {
            Tab::Unified => self.data.correlated.len(),
            Tab::Platform => self.data.events.len(),
            Tab::Health => self.data.health_scores.len(),
            Tab::Settings => self.data.platform_health.len(),
            Tab::Logs => {
                if self.log_filter == LogFilter::All {
                    self.log_buffer.len()
                } else {
                    self.log_buffer
                        .snapshot()
                        .iter()
                        .filter(|e| self.log_filter.matches(&e.level))
                        .count()
                }
            }
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

    pub fn selected_settings_platform(&self) -> PlatformKind {
        if self.data.platform_health.is_empty() {
            return PlatformKind::ALL[self.selected_row.min(PlatformKind::ALL.len() - 1)];
        }
        self.data
            .platform_health
            .get(self.selected_row.min(self.data.platform_health.len() - 1))
            .map(|report| report.platform)
            .unwrap_or(PlatformKind::GitHub)
    }

    pub fn selected_settings_report(&self) -> Option<&PlatformHealthReport> {
        if self.data.platform_health.is_empty() {
            return None;
        }
        self.data.platform_health.get(
            self.selected_row
                .min(self.data.platform_health.len().saturating_sub(1)),
        )
    }

    pub fn selected_token_from_env(&self) -> bool {
        let Some(source) = self
            .selected_settings_report()
            .and_then(|report| report.token_source.as_deref())
        else {
            return false;
        };

        if source.eq_ignore_ascii_case("keyring") || source.to_ascii_lowercase().contains("cli") {
            return false;
        }

        source
            .chars()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
    }

    pub fn queue_action(&mut self, request: ActionRequest, next_state: SettingsFlowState) {
        self.pending_action = Some(request);
        self.settings_action_in_flight = true;
        self.settings_flow = next_state;
    }

    pub fn take_pending_action(&mut self) -> Option<ActionRequest> {
        self.pending_action.take()
    }

    pub fn handle_action_result(&mut self, result: ActionResult) -> ActionOutcome {
        self.settings_action_in_flight = false;
        let mut outcome = ActionOutcome::default();

        match result {
            ActionResult::TokenStored {
                platform,
                identity,
                warnings,
            } => {
                self.settings_message = Some(format!(
                    "ok: {} token saved ({identity})",
                    platform.display_name()
                ));
                if !warnings.is_empty() {
                    self.settings_message = Some(format!(
                        "warn: {} token saved with warnings: {}",
                        platform.display_name(),
                        warnings.join("; ")
                    ));
                }
                self.token_input.clear();
                self.settings_flow = SettingsFlowState::ValidationResult;
                outcome.force_refresh = true;
            }
            ActionResult::TokenRemoved { platform } => {
                self.settings_message =
                    Some(format!("ok: {} token removed", platform.display_name()));
                self.settings_flow = SettingsFlowState::ValidationResult;
                outcome.force_refresh = true;
            }
            ActionResult::PlatformValidated {
                platform,
                identity,
                warnings,
            } => {
                if warnings.is_empty() {
                    self.settings_message = Some(format!(
                        "ok: {} token valid ({identity})",
                        platform.display_name()
                    ));
                } else {
                    self.settings_message = Some(format!(
                        "warn: {} token valid ({identity}) with warnings: {}",
                        platform.display_name(),
                        warnings.join("; ")
                    ));
                }
                self.settings_flow = SettingsFlowState::ValidationResult;
                outcome.force_refresh = true;
            }
            ActionResult::DiscoveryCompleted { payload } => {
                let warning_count = payload.warnings.len();
                self.onboarding.set_discovery(payload);
                self.settings_flow = SettingsFlowState::ResourceSelection;
                if warning_count > 0 {
                    self.settings_message = Some(format!(
                        "warn: discovery completed with {warning_count} warning(s)"
                    ));
                } else {
                    self.settings_message = Some("ok: discovery completed".to_string());
                }
            }
            ActionResult::CorrelationPreview { lines } => {
                self.onboarding.correlation_preview = lines;
                self.settings_flow = SettingsFlowState::CorrelationReview;
            }
            ActionResult::CorrelationsApplied {
                added,
                updated,
                total,
                config,
            } => {
                self.settings_message = Some(format!(
                    "ok: saved correlations: {total} total ({added} new, {updated} updated)"
                ));
                self.settings_flow = SettingsFlowState::ValidationResult;
                outcome.replace_config = Some(config);
                outcome.force_refresh = true;
                self.onboarding.reset();
            }
            ActionResult::Error { context, message } => {
                self.settings_message = Some(format!("error: {context}: {message}"));
                self.settings_flow = SettingsFlowState::ValidationResult;
                self.last_error = Some(message);
            }
        }

        outcome
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
        assert_eq!(Tab::from_index(3), Tab::Settings);
        assert_eq!(Tab::from_index(4), Tab::Logs);
        assert_eq!(Tab::from_index(5), Tab::Unified);
    }

    #[test]
    fn tab_next_prev_round_trip() {
        let tab = Tab::Unified;
        assert_eq!(tab.next(), Tab::Platform);
        assert_eq!(tab.next().next(), Tab::Health);
        assert_eq!(tab.next().next().next(), Tab::Settings);
        assert_eq!(tab.next().next().next().next(), Tab::Logs);
        assert_eq!(tab.next().next().next().next().next(), Tab::Unified);

        assert_eq!(tab.prev(), Tab::Logs);
        assert_eq!(tab.prev().prev(), Tab::Settings);
        assert_eq!(tab.prev().prev().prev(), Tab::Health);
        assert_eq!(tab.prev().prev().prev().prev(), Tab::Platform);
        assert_eq!(tab.prev().prev().prev().prev().prev(), Tab::Unified);
    }

    #[test]
    fn tab_labels() {
        assert_eq!(Tab::Unified.label(), "Unified Overview");
        assert_eq!(Tab::Platform.label(), "Platform Details");
        assert_eq!(Tab::Health.label(), "Health & Metrics");
        assert_eq!(Tab::Settings.label(), "Settings");
        assert_eq!(Tab::Logs.label(), "Logs");
    }

    #[test]
    fn tab_short_labels() {
        assert_eq!(Tab::Unified.short_label(), "Unified");
        assert_eq!(Tab::Platform.short_label(), "Platform");
        assert_eq!(Tab::Health.short_label(), "Health");
        assert_eq!(Tab::Settings.short_label(), "Settings");
        assert_eq!(Tab::Logs.short_label(), "Logs");
    }

    #[test]
    fn app_default_tab_from_config() {
        let data = DataSnapshot::default();

        let config = TuiConfig::default();
        let app = App::new(data.clone(), config, LogRingBuffer::new());
        assert_eq!(app.active_tab, Tab::Unified);

        let mut config = TuiConfig::default();
        config.default_tab = "platform".into();
        let app = App::new(data.clone(), config, LogRingBuffer::new());
        assert_eq!(app.active_tab, Tab::Platform);

        let mut config = TuiConfig::default();
        config.default_tab = "health".into();
        let app = App::new(data.clone(), config, LogRingBuffer::new());
        assert_eq!(app.active_tab, Tab::Health);

        let mut config = TuiConfig::default();
        config.default_tab = "settings".into();
        let app = App::new(data, config, LogRingBuffer::new());
        assert_eq!(app.active_tab, Tab::Settings);
    }

    #[test]
    fn app_clamp_selection() {
        let data = DataSnapshot::default();
        let mut app = App::new(data, TuiConfig::default(), LogRingBuffer::new());

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
        assert!(snap.health_breakdowns.is_empty());
        assert!(snap.warnings.is_empty());
    }
}
