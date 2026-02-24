//! TUI application state — the single source of truth for the UI.

use std::collections::{HashMap, HashSet};

use chrono::{DateTime, Utc};
use pulsos_core::auth::PlatformKind;
use pulsos_core::config::types::PulsosConfig;
use pulsos_core::config::types::TuiConfig;
use pulsos_core::domain::analytics::DoraMetrics;
use pulsos_core::domain::deployment::{DeploymentEvent, DeploymentStatus, JobDetail};
use pulsos_core::domain::health::HealthBreakdown;
use pulsos_core::domain::metrics::ProjectTelemetry;
use pulsos_core::domain::project::CorrelatedEvent;
use pulsos_core::health::PlatformHealthReport;
use serde::{Deserialize, Serialize};

use super::actions::{ActionRequest, ActionResult};
use super::log_buffer::{LogEntry, LogRingBuffer};
use super::settings_flow::{OnboardingState, SettingsFlowState};
use super::theme::Theme;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PlatformSubtab {
    GitHub,
    Railway,
    Vercel,
}

impl PlatformSubtab {
    pub fn next(self) -> Self {
        match self {
            Self::GitHub => Self::Railway,
            Self::Railway => Self::Vercel,
            Self::Vercel => Self::GitHub,
        }
    }

    pub fn prev(self) -> Self {
        match self {
            Self::GitHub => Self::Vercel,
            Self::Railway => Self::GitHub,
            Self::Vercel => Self::Railway,
        }
    }

    pub fn short_label(self) -> &'static str {
        match self {
            Self::GitHub => "GH",
            Self::Railway => "RW",
            Self::Vercel => "VC",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HealthGroup {
    Railway,
    Vercel,
    GitHubOnly,
}

impl HealthGroup {
    pub const ALL: [HealthGroup; 3] = [
        HealthGroup::Railway,
        HealthGroup::Vercel,
        HealthGroup::GitHubOnly,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Railway => "Railway Projects",
            Self::Vercel => "Vercel Projects",
            Self::GitHubOnly => "GitHub-only Projects",
        }
    }
}

#[derive(Debug, Clone)]
pub struct HealthProjectRow {
    pub group: HealthGroup,
    pub name: String,
    pub score: u8,
}

#[derive(Debug, Clone, Copy)]
pub struct PlatformLatestRow {
    pub event_idx: usize,
}

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

/// Sort order for the Unified Overview table.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum UnifiedSort {
    /// Newest event first (default).
    #[default]
    ByTime,
    /// Group by CD platform present: Railway → Vercel → GitHub-only → Unmatched.
    ByPlatform,
}

impl UnifiedSort {
    pub fn next(self) -> Self {
        match self {
            Self::ByTime => Self::ByPlatform,
            Self::ByPlatform => Self::ByTime,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::ByTime => "time",
            Self::ByPlatform => "platform",
        }
    }
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DetailsFocus {
    LeftTree,
    RightPanel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TreeItemKind {
    RunHeader,
    Job {
        job_id: u64,
        job_idx: usize,
    },
    Step {
        job_id: u64,
        job_idx: usize,
        step_idx: usize,
    },
}

#[derive(Debug, Clone)]
pub struct TreeItem {
    pub kind: TreeItemKind,
    pub depth: u8,
    pub label: String,
    pub status: DeploymentStatus,
}

#[derive(Debug, Clone)]
pub enum RightContent {
    Summary { lines: Vec<String> },
    Error { message: String },
}

#[derive(Debug, Clone)]
pub struct PlatformDetailsState {
    pub anchor_event_index: usize,
    pub anchor_event_id: String,
    pub repo: String,
    pub run_id: u64,
    pub focus: DetailsFocus,
    pub tree_items: Vec<TreeItem>,
    pub tree_cursor: usize,
    pub expanded_jobs: HashSet<u64>,
    pub tree_scroll: usize,
    pub right_scroll: usize,
    pub right_content: RightContent,
    pub show_logs: bool,
}

/// A snapshot of all data needed to render the TUI.
///
/// Produced by the background poller and consumed by the renderer.
/// Intentionally `Clone` — data volumes are small (tens of events).
#[derive(Debug, Clone, Serialize, Deserialize)]
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
    /// Per-project UI grouping for the Health tab.
    #[serde(default)]
    pub health_project_groups: Vec<(String, HealthGroup)>,
    /// Warnings from platform fetches.
    pub warnings: Vec<String>,
    /// Per-platform setup/auth/connectivity readiness reports.
    pub platform_health: Vec<PlatformHealthReport>,
    /// Per-project real-time telemetry (keyed by correlation name).
    ///
    /// Populated by the background poller — Railway container stats and
    /// Ping Engine results. Empty until the first telemetry cycle completes.
    pub telemetry: HashMap<String, ProjectTelemetry>,
    /// Aggregated DORA metrics computed over the session-level history buffer.
    pub dora_metrics: DoraMetrics,
    /// Number of correlated events accumulated in the DORA history buffer.
    pub dora_history_count: usize,
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
            health_project_groups: Vec::new(),
            warnings: Vec::new(),
            platform_health: Vec::new(),
            telemetry: HashMap::new(),
            dora_metrics: DoraMetrics::default(),
            dora_history_count: 0,
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
    /// Current sort order for the Unified Overview table.
    pub unified_sort: UnifiedSort,
    /// Platform tab details-mode state (GitHub tree + right panel).
    pub platform_details_mode: Option<PlatformDetailsState>,
    /// Active provider subtab inside the Platform tab.
    pub platform_subtab: PlatformSubtab,
    /// True when the general settings panel is open.
    pub settings_general_mode: bool,
    /// Cursor row inside the general settings panel (0=daemon, 1=theme, 2=fps).
    pub settings_general_cursor: usize,
    /// Live theme for the current session.
    pub theme: Theme,
    /// Whether the daemon process is currently running (polled on each Tick in Settings tab).
    pub daemon_running: bool,
}

impl App {
    pub fn new(data: DataSnapshot, tui_config: TuiConfig, log_buffer: LogRingBuffer) -> Self {
        let default_tab = match tui_config.default_tab.as_str() {
            "by_platform" | "platform" => Tab::Platform,
            "health" => Tab::Health,
            "settings" => Tab::Settings,
            _ => Tab::Unified,
        };
        let theme = Theme::resolve(&tui_config.theme);

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
            unified_sort: UnifiedSort::default(),
            platform_details_mode: None,
            platform_subtab: PlatformSubtab::GitHub,
            settings_general_mode: false,
            settings_general_cursor: 0,
            theme,
            daemon_running: false,
        }
    }

    /// Number of displayable rows in the current tab.
    pub fn row_count(&self) -> usize {
        match self.active_tab {
            Tab::Unified => self.data.correlated.len(),
            Tab::Platform => self.platform_row_count(),
            Tab::Health => self.health_grouped_rows().len(),
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

    pub fn platform_details_active(&self) -> bool {
        self.active_tab == Tab::Platform
            && self
                .platform_details_mode
                .as_ref()
                .map(|details| details.show_logs)
                .unwrap_or(false)
    }

    pub fn close_platform_details_mode(&mut self) {
        self.platform_details_mode = None;
    }

    fn hide_platform_logs_panel(&mut self) {
        if let Some(details) = self.platform_details_mode.as_mut() {
            details.show_logs = false;
            details.focus = DetailsFocus::LeftTree;
            details.right_scroll = 0;
        }
    }

    pub fn set_platform_subtab(&mut self, subtab: PlatformSubtab) {
        if self.platform_subtab == subtab {
            return;
        }
        self.platform_subtab = subtab;
        self.close_platform_details_mode();
        self.selected_row = 0;
        self.clamp_selection();
    }

    pub fn next_platform_subtab(&mut self) {
        self.set_platform_subtab(self.platform_subtab.next());
    }

    pub fn prev_platform_subtab(&mut self) {
        self.set_platform_subtab(self.platform_subtab.prev());
    }

    fn platform_row_count(&self) -> usize {
        match self.platform_subtab {
            PlatformSubtab::GitHub => self.platform_filtered_event_indices().len(),
            PlatformSubtab::Railway | PlatformSubtab::Vercel => self.platform_latest_rows().len(),
        }
    }

    pub fn platform_filtered_event_indices(&self) -> Vec<usize> {
        let q = self.search_query.to_ascii_lowercase();
        self.data
            .events
            .iter()
            .enumerate()
            .filter(|(_, e)| {
                let matches_platform = match self.platform_subtab {
                    PlatformSubtab::GitHub => {
                        e.platform == pulsos_core::domain::deployment::Platform::GitHub
                    }
                    PlatformSubtab::Railway => {
                        e.platform == pulsos_core::domain::deployment::Platform::Railway
                    }
                    PlatformSubtab::Vercel => {
                        e.platform == pulsos_core::domain::deployment::Platform::Vercel
                    }
                };
                matches_platform && (q.is_empty() || Self::platform_event_matches_query(e, &q))
            })
            .map(|(i, _)| i)
            .collect()
    }

    pub fn platform_latest_rows(&self) -> Vec<PlatformLatestRow> {
        let mut by_source: HashMap<String, usize> = HashMap::new();
        for event_idx in self.platform_filtered_event_indices() {
            let Some(event) = self.data.events.get(event_idx) else {
                continue;
            };
            let key = event
                .metadata
                .source_id
                .clone()
                .unwrap_or_else(|| event.id.clone());
            match by_source.get(&key).copied() {
                Some(existing_idx) => {
                    let existing = &self.data.events[existing_idx];
                    if event.created_at > existing.created_at {
                        by_source.insert(key, event_idx);
                    }
                }
                None => {
                    by_source.insert(key, event_idx);
                }
            }
        }
        let mut rows: Vec<PlatformLatestRow> = by_source
            .into_values()
            .map(|event_idx| PlatformLatestRow { event_idx })
            .collect();
        rows.sort_by(|a, b| {
            let left = self.data.events[a.event_idx].created_at;
            let right = self.data.events[b.event_idx].created_at;
            right.cmp(&left)
        });
        rows
    }

    fn build_platform_details_state(
        event: &DeploymentEvent,
        anchor_event_index: usize,
        show_logs: bool,
    ) -> PlatformDetailsState {
        let run_id = event.id.parse::<u64>().unwrap_or(0);
        let repo = event.metadata.source_id.clone().unwrap_or_default();
        let expanded_jobs = HashSet::new();
        let tree_items = Self::build_tree_items(event, &expanded_jobs);
        let mut right_content = RightContent::Summary {
            lines: Self::run_summary_lines(event, run_id, &repo),
        };
        if run_id == 0 || repo.is_empty() {
            right_content = RightContent::Error {
                message: "Run metadata incomplete (missing repo or non-numeric run id)."
                    .to_string(),
            };
        }
        PlatformDetailsState {
            anchor_event_index,
            anchor_event_id: event.id.clone(),
            repo,
            run_id,
            focus: DetailsFocus::LeftTree,
            tree_items,
            tree_cursor: 0,
            expanded_jobs,
            tree_scroll: 0,
            right_scroll: 0,
            right_content,
            show_logs,
        }
    }

    /// Ensure GH tree state exists for the selected GH row.
    ///
    /// Tree is always shown by default in GH subtab; logs panel remains hidden
    /// until explicitly toggled.
    pub fn ensure_platform_tree_state(&mut self) {
        if self.active_tab != Tab::Platform || self.platform_subtab != PlatformSubtab::GitHub {
            self.platform_details_mode = None;
            return;
        }
        let indices = self.platform_filtered_event_indices();
        if indices.is_empty() {
            self.platform_details_mode = None;
            return;
        }
        let filtered_idx = self.selected_row.min(indices.len().saturating_sub(1));
        let Some(event) = self.data.events.get(indices[filtered_idx]) else {
            self.platform_details_mode = None;
            return;
        };
        if event.platform != pulsos_core::domain::deployment::Platform::GitHub {
            self.platform_details_mode = None;
            return;
        }
        let show_logs = self
            .platform_details_mode
            .as_ref()
            .map(|details| details.show_logs)
            .unwrap_or(false);
        let needs_replace = self
            .platform_details_mode
            .as_ref()
            .map(|details| details.anchor_event_id != event.id)
            .unwrap_or(true);
        if needs_replace {
            self.platform_details_mode = Some(Self::build_platform_details_state(
                event,
                filtered_idx,
                show_logs,
            ));
        } else if let Some(details) = self.platform_details_mode.as_mut() {
            details.anchor_event_index = filtered_idx;
        }
    }

    /// Toggle the GH logs/details right pane while keeping the left tree visible.
    pub fn toggle_platform_logs_panel(&mut self) {
        if self.platform_subtab != PlatformSubtab::GitHub {
            return;
        }
        self.ensure_platform_tree_state();
        let Some(details) = self.platform_details_mode.as_mut() else {
            return;
        };
        details.show_logs = !details.show_logs;
        details.focus = DetailsFocus::LeftTree;
        if !details.show_logs {
            details.right_scroll = 0;
        }
    }

    pub fn sync_platform_details_with_data(&mut self) {
        if self.platform_subtab != PlatformSubtab::GitHub {
            self.platform_details_mode = None;
            return;
        }
        let Some(mut details) = self.platform_details_mode.take() else {
            return;
        };

        let indices = self.platform_filtered_event_indices();
        let Some((filtered_idx, event_idx)) = indices
            .iter()
            .enumerate()
            .find(|(_, idx)| self.data.events[**idx].id == details.anchor_event_id)
            .map(|(i, idx)| (i, *idx))
        else {
            self.platform_details_mode = None;
            return;
        };

        details.anchor_event_index = filtered_idx;
        let event = &self.data.events[event_idx];
        details.tree_items = Self::build_tree_items(event, &details.expanded_jobs);
        if details.tree_items.is_empty() {
            details.tree_items.push(TreeItem {
                kind: TreeItemKind::RunHeader,
                depth: 0,
                label: "run".to_string(),
                status: event.status.clone(),
            });
        }
        if details.tree_cursor >= details.tree_items.len() {
            details.tree_cursor = details.tree_items.len().saturating_sub(1);
        }
        self.platform_details_mode = Some(details);
    }

    pub fn details_move_tree_cursor(&mut self, delta: i32) {
        let Some(details) = self.platform_details_mode.as_mut() else {
            return;
        };
        if details.tree_items.is_empty() {
            details.tree_cursor = 0;
            return;
        }
        let max = details.tree_items.len().saturating_sub(1) as i32;
        let next = (details.tree_cursor as i32 + delta).clamp(0, max) as usize;
        details.tree_cursor = next;
        if details.tree_cursor < details.tree_scroll {
            details.tree_scroll = details.tree_cursor;
        } else {
            details.tree_scroll = details.tree_cursor.saturating_sub(6);
        }
    }

    pub fn details_focus_right(&mut self) {
        if let Some(details) = self.platform_details_mode.as_mut() {
            details.focus = DetailsFocus::RightPanel;
        }
    }

    pub fn details_scroll_right(&mut self, delta: i32) {
        let Some(details) = self.platform_details_mode.as_mut() else {
            return;
        };
        let next = (details.right_scroll as i32 + delta).max(0) as usize;
        details.right_scroll = next;
    }

    pub fn details_toggle_or_open_right(&mut self) {
        let Some(item) = self
            .platform_details_mode
            .as_ref()
            .and_then(|details| details.tree_items.get(details.tree_cursor))
            .cloned()
        else {
            return;
        };

        match item.kind {
            TreeItemKind::RunHeader => {
                let event = self.anchored_platform_event().cloned();
                if let Some(details) = self.platform_details_mode.as_mut() {
                    details.focus = DetailsFocus::RightPanel;
                    details.right_scroll = 0;
                    if let Some(event) = event {
                        details.right_content = RightContent::Summary {
                            lines: Self::run_summary_lines(&event, details.run_id, &details.repo),
                        };
                    }
                }
            }
            TreeItemKind::Job { job_id, job_idx } => {
                let should_expand = self
                    .platform_details_mode
                    .as_ref()
                    .map(|details| !details.expanded_jobs.contains(&job_id))
                    .unwrap_or(false);
                if should_expand {
                    let event = self.anchored_platform_event().cloned();
                    if let Some(details) = self.platform_details_mode.as_mut() {
                        details.expanded_jobs.insert(job_id);
                        if let Some(event) = event {
                            details.tree_items =
                                Self::build_tree_items(&event, &details.expanded_jobs);
                            if details.tree_cursor >= details.tree_items.len() {
                                details.tree_cursor = details.tree_items.len().saturating_sub(1);
                            }
                            details.tree_scroll = details.tree_cursor.saturating_sub(6);
                        }
                    }
                    return;
                }
                // Job is expanded — show job summary synchronously (dropdown mode).
                let event = self.anchored_platform_event().cloned();
                if let Some(details) = self.platform_details_mode.as_mut() {
                    details.focus = DetailsFocus::RightPanel;
                    details.right_scroll = 0;
                    if let Some(event) = &event {
                        if let Some(job) = event.metadata.job_details.get(job_idx) {
                            details.right_content = RightContent::Summary {
                                lines: Self::job_summary_lines(job, None),
                            };
                        } else if let Some(summary_job) = event.metadata.jobs.get(job_idx) {
                            details.right_content = RightContent::Summary {
                                lines: vec![
                                    format!("Job: {}", summary_job.name),
                                    format!("Status: {}", summary_job.status),
                                ],
                            };
                        }
                    }
                }
            }
            TreeItemKind::Step {
                job_idx, step_idx, ..
            } => {
                // Show step summary synchronously (dropdown mode).
                let event = self.anchored_platform_event().cloned();
                if let Some(details) = self.platform_details_mode.as_mut() {
                    details.focus = DetailsFocus::RightPanel;
                    details.right_scroll = 0;
                    if let Some(event) = &event {
                        if let Some(job) = event.metadata.job_details.get(job_idx) {
                            details.right_content = RightContent::Summary {
                                lines: Self::job_summary_lines(job, Some(step_idx)),
                            };
                        }
                    }
                }
            }
        }
    }

    pub fn details_left_action(&mut self) {
        let Some(focus) = self
            .platform_details_mode
            .as_ref()
            .map(|details| details.focus)
        else {
            return;
        };
        if focus == DetailsFocus::RightPanel {
            if let Some(details) = self.platform_details_mode.as_mut() {
                details.focus = DetailsFocus::LeftTree;
            }
            return;
        }

        let Some(item) = self
            .platform_details_mode
            .as_ref()
            .and_then(|details| details.tree_items.get(details.tree_cursor))
            .cloned()
        else {
            return;
        };

        match item.kind {
            TreeItemKind::RunHeader => self.hide_platform_logs_panel(),
            TreeItemKind::Job { job_id, .. } => {
                let is_expanded = self
                    .platform_details_mode
                    .as_ref()
                    .map(|details| details.expanded_jobs.contains(&job_id))
                    .unwrap_or(false);
                if is_expanded {
                    let event = self.anchored_platform_event().cloned();
                    if let Some(details) = self.platform_details_mode.as_mut() {
                        details.expanded_jobs.remove(&job_id);
                        if let Some(event) = event {
                            details.tree_items =
                                Self::build_tree_items(&event, &details.expanded_jobs);
                            if details.tree_cursor >= details.tree_items.len() {
                                details.tree_cursor = details.tree_items.len().saturating_sub(1);
                            }
                            details.tree_scroll = details.tree_cursor.saturating_sub(6);
                        }
                    }
                } else {
                    self.hide_platform_logs_panel();
                }
            }
            TreeItemKind::Step { job_id, .. } => {
                let job_idx = self.platform_details_mode.as_ref().and_then(|details| {
                    details
                        .tree_items
                        .iter()
                        .position(|it| matches!(it.kind, TreeItemKind::Job { job_id: jid, .. } if jid == job_id))
                });
                if let Some(job_idx) = job_idx {
                    if let Some(details) = self.platform_details_mode.as_mut() {
                        details.tree_cursor = job_idx;
                        details.tree_scroll = details.tree_cursor.saturating_sub(6);
                    }
                }
            }
        }
    }

    pub fn details_current_focus(&self) -> Option<DetailsFocus> {
        self.platform_details_mode.as_ref().map(|d| d.focus)
    }

    pub fn details_state(&self) -> Option<&PlatformDetailsState> {
        self.platform_details_mode.as_ref()
    }

    fn anchored_platform_event(&self) -> Option<&DeploymentEvent> {
        let details = self.platform_details_mode.as_ref()?;
        self.data
            .events
            .iter()
            .find(|e| e.id == details.anchor_event_id)
    }

    fn platform_event_matches_query(e: &DeploymentEvent, q: &str) -> bool {
        e.title
            .as_deref()
            .unwrap_or("")
            .to_ascii_lowercase()
            .contains(q)
            || e.platform.to_string().to_ascii_lowercase().contains(q)
            || e.branch
                .as_deref()
                .unwrap_or("")
                .to_ascii_lowercase()
                .contains(q)
            || e.actor
                .as_deref()
                .unwrap_or("")
                .to_ascii_lowercase()
                .contains(q)
            || e.metadata
                .source_id
                .as_deref()
                .unwrap_or("")
                .to_ascii_lowercase()
                .contains(q)
    }

    fn infer_health_group(&self, project: &str) -> HealthGroup {
        if let Some((_, group)) = self
            .data
            .health_project_groups
            .iter()
            .find(|(name, _)| name == project)
        {
            return *group;
        }
        let Some((_, breakdown)) = self
            .data
            .health_breakdowns
            .iter()
            .find(|(name, _)| name == project)
        else {
            return HealthGroup::GitHubOnly;
        };
        if breakdown.railway_score.is_some() || breakdown.railway_weight > 0 {
            HealthGroup::Railway
        } else if breakdown.vercel_score.is_some() || breakdown.vercel_weight > 0 {
            HealthGroup::Vercel
        } else {
            HealthGroup::GitHubOnly
        }
    }

    pub fn health_grouped_rows(&self) -> Vec<HealthProjectRow> {
        let mut railway = Vec::new();
        let mut vercel = Vec::new();
        let mut github_only = Vec::new();
        for (name, score) in &self.data.health_scores {
            let row = HealthProjectRow {
                group: self.infer_health_group(name),
                name: name.clone(),
                score: *score,
            };
            match row.group {
                HealthGroup::Railway => railway.push(row),
                HealthGroup::Vercel => vercel.push(row),
                HealthGroup::GitHubOnly => github_only.push(row),
            }
        }
        let mut rows = railway;
        rows.extend(vercel);
        rows.extend(github_only);
        rows
    }

    pub fn selected_health_project(&self) -> Option<HealthProjectRow> {
        let rows = self.health_grouped_rows();
        if rows.is_empty() {
            return None;
        }
        Some(rows[self.selected_row.min(rows.len().saturating_sub(1))].clone())
    }

    pub fn selected_log_entry(&self) -> Option<LogEntry> {
        let filtered: Vec<LogEntry> = self
            .log_buffer
            .snapshot()
            .into_iter()
            .filter(|entry| self.log_filter.matches(&entry.level))
            .collect();
        if filtered.is_empty() {
            return None;
        }
        Some(filtered[self.selected_row.min(filtered.len().saturating_sub(1))].clone())
    }

    fn synthetic_job_id(job_idx: usize) -> u64 {
        10_000_000_000_u64 + job_idx as u64
    }

    fn build_tree_items(event: &DeploymentEvent, expanded_jobs: &HashSet<u64>) -> Vec<TreeItem> {
        let mut items = vec![TreeItem {
            kind: TreeItemKind::RunHeader,
            depth: 0,
            label: event
                .metadata
                .workflow_name
                .clone()
                .or_else(|| event.title.clone())
                .unwrap_or_else(|| format!("run {}", event.id)),
            status: event.status.clone(),
        }];

        if !event.metadata.job_details.is_empty() {
            for (job_idx, job) in event.metadata.job_details.iter().enumerate() {
                let job_id = job
                    .job_id
                    .unwrap_or_else(|| Self::synthetic_job_id(job_idx));
                items.push(TreeItem {
                    kind: TreeItemKind::Job { job_id, job_idx },
                    depth: 1,
                    label: job.name.clone(),
                    status: job.status.clone(),
                });
                if expanded_jobs.contains(&job_id) {
                    for (step_idx, step) in job.steps.iter().enumerate() {
                        items.push(TreeItem {
                            kind: TreeItemKind::Step {
                                job_id,
                                job_idx,
                                step_idx,
                            },
                            depth: 2,
                            label: format!("{}. {}", step.number, step.name),
                            status: step.status.clone(),
                        });
                    }
                }
            }
            return items;
        }

        if !event.metadata.jobs.is_empty() {
            for (job_idx, job) in event.metadata.jobs.iter().enumerate() {
                let job_id = Self::synthetic_job_id(job_idx);
                items.push(TreeItem {
                    kind: TreeItemKind::Job { job_id, job_idx },
                    depth: 1,
                    label: job.name.clone(),
                    status: job.status.clone(),
                });
            }
        }

        items
    }

    fn run_summary_lines(event: &DeploymentEvent, run_id: u64, repo: &str) -> Vec<String> {
        let mut lines = vec![
            format!(
                "Run: {}",
                if run_id == 0 {
                    event.id.clone()
                } else {
                    run_id.to_string()
                }
            ),
            format!("Repo: {}", if repo.is_empty() { "-" } else { repo }),
            format!(
                "Workflow: {}",
                event.metadata.workflow_name.as_deref().unwrap_or("-")
            ),
            format!(
                "Trigger: {}",
                event.metadata.trigger_event.as_deref().unwrap_or("-")
            ),
            format!("Status: {}", event.status),
            format!("Actor: {}", event.actor.as_deref().unwrap_or("-")),
        ];
        if let Some(branch) = &event.branch {
            lines.push(format!("Branch: {branch}"));
        }
        if let Some(url) = &event.url {
            lines.push(format!("URL: {url}"));
        }
        lines
    }

    fn job_summary_lines(job: &JobDetail, step_idx: Option<usize>) -> Vec<String> {
        let mut lines = vec![
            format!("Job: {}", job.name),
            format!("Status: {}", job.status),
            format!("Steps: {}", job.steps.len()),
        ];
        if let Some(url) = &job.html_url {
            lines.push(format!("URL: {url}"));
        }
        if let Some(step_idx) = step_idx.and_then(|i| job.steps.get(i)) {
            lines.push(format!("Step: {}. {}", step_idx.number, step_idx.name));
            if let Some(started) = step_idx.started_at {
                lines.push(format!("Step started: {started}"));
            }
            if let Some(completed) = step_idx.completed_at {
                lines.push(format!("Step completed: {completed}"));
            }
        }
        lines
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

    /// Refresh `daemon_running` by checking whether the daemon PID is alive.
    pub fn refresh_daemon_status(&mut self) {
        self.daemon_running = is_daemon_running();
    }

    pub fn handle_action_result(&mut self, result: ActionResult) -> ActionOutcome {
        let mut outcome = ActionOutcome::default();

        self.settings_action_in_flight = false;

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
                outcome.replace_config = Some(*config);
                outcome.force_refresh = true;
                self.onboarding.reset();
            }
            ActionResult::DaemonStarted => {
                self.daemon_running = true;
                self.settings_message = Some("daemon started".to_string());
                self.settings_flow = SettingsFlowState::Idle;
            }
            ActionResult::DaemonStopped => {
                self.daemon_running = false;
                self.settings_message = Some("daemon stopped".to_string());
                self.settings_flow = SettingsFlowState::Idle;
            }
            ActionResult::DaemonAlreadyRunning => {
                self.daemon_running = true;
                self.settings_message = Some("daemon is already running".to_string());
                self.settings_flow = SettingsFlowState::Idle;
            }
            ActionResult::DaemonNotRunning => {
                self.daemon_running = false;
                self.settings_message = Some("daemon is not running".to_string());
                self.settings_flow = SettingsFlowState::Idle;
            }
            ActionResult::TuiConfigSaved => {
                self.settings_flow = SettingsFlowState::Idle;
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

/// Check whether the daemon process is alive by reading its PID file.
///
/// On Unix this uses `/proc/<pid>` (Linux) or a stderr-suppressed `kill -0`
/// (macOS). Stderr is always redirected to null so stale-PID messages never
/// leak into the TUI. When the process is confirmed dead the stale PID file
/// is removed so subsequent Tick polls skip the subprocess entirely.
fn is_daemon_running() -> bool {
    let Some(path) = dirs::config_dir().map(|d| d.join("pulsos").join("daemon.pid")) else {
        return false;
    };
    let Ok(content) = std::fs::read_to_string(&path) else {
        return false;
    };
    let Ok(pid) = content.trim().parse::<u32>() else {
        return false;
    };

    #[cfg(unix)]
    {
        let alive = std::process::Command::new("kill")
            .args(["-0", &pid.to_string()])
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null()) // suppress "No such process"
            .status()
            .map(|s| s.success())
            .unwrap_or(false);

        if !alive {
            // Remove the stale PID file so future Tick polls skip this check.
            let _ = std::fs::remove_file(&path);
        }
        alive
    }
    #[cfg(not(unix))]
    {
        let _ = pid;
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration, Utc};
    use pulsos_core::domain::deployment::{
        DeploymentEvent, DeploymentStatus, EventMetadata, Platform,
    };

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

        let config = TuiConfig {
            default_tab: "platform".into(),
            ..Default::default()
        };
        let app = App::new(data.clone(), config, LogRingBuffer::new());
        assert_eq!(app.active_tab, Tab::Platform);

        let config = TuiConfig {
            default_tab: "health".into(),
            ..Default::default()
        };
        let app = App::new(data.clone(), config, LogRingBuffer::new());
        assert_eq!(app.active_tab, Tab::Health);

        let config = TuiConfig {
            default_tab: "settings".into(),
            ..Default::default()
        };
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

    #[test]
    fn platform_latest_rows_keeps_newest_per_source_id() {
        let now = Utc::now();
        let data = DataSnapshot {
            events: vec![
                DeploymentEvent {
                    id: "rw-older".into(),
                    platform: Platform::Railway,
                    status: DeploymentStatus::Success,
                    commit_sha: Some("1111111".into()),
                    branch: Some("main".into()),
                    title: Some("old".into()),
                    actor: None,
                    created_at: now - Duration::minutes(5),
                    updated_at: None,
                    duration_secs: Some(10),
                    url: Some("https://old.example.com".into()),
                    metadata: EventMetadata {
                        source_id: Some("proj:svc:prod".into()),
                        ..Default::default()
                    },
                    is_from_cache: false,
                },
                DeploymentEvent {
                    id: "rw-newer".into(),
                    platform: Platform::Railway,
                    status: DeploymentStatus::Success,
                    commit_sha: Some("2222222".into()),
                    branch: Some("main".into()),
                    title: Some("new".into()),
                    actor: None,
                    created_at: now,
                    updated_at: None,
                    duration_secs: Some(8),
                    url: Some("https://new.example.com".into()),
                    metadata: EventMetadata {
                        source_id: Some("proj:svc:prod".into()),
                        ..Default::default()
                    },
                    is_from_cache: false,
                },
            ],
            ..Default::default()
        };
        let mut app = App::new(data, TuiConfig::default(), LogRingBuffer::new());
        app.active_tab = Tab::Platform;
        app.platform_subtab = PlatformSubtab::Railway;
        let rows = app.platform_latest_rows();
        assert_eq!(rows.len(), 1);
        let event = app.data.events.get(rows[0].event_idx).expect("event");
        assert_eq!(event.id, "rw-newer");
    }

    #[test]
    fn health_grouped_rows_follow_provider_precedence() {
        let data = DataSnapshot {
            health_scores: vec![
                ("rw-project".into(), 92),
                ("vc-project".into(), 84),
                ("gh-project".into(), 76),
            ],
            health_project_groups: vec![
                ("rw-project".into(), HealthGroup::Railway),
                ("vc-project".into(), HealthGroup::Vercel),
                ("gh-project".into(), HealthGroup::GitHubOnly),
            ],
            ..Default::default()
        };
        let app = App::new(data, TuiConfig::default(), LogRingBuffer::new());
        let rows = app.health_grouped_rows();
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0].name, "rw-project");
        assert_eq!(rows[1].name, "vc-project");
        assert_eq!(rows[2].name, "gh-project");
    }
}
