//! Keybinding dispatch — maps keyboard input to App mutations.

use super::actions::ActionRequest;
use super::app::{App, DetailsFocus, InputMode, PlatformSubtab, Tab};
use super::log_buffer::LogEntry;
use super::settings_flow::SettingsFlowState;
use chrono::Utc;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
#[cfg(not(test))]
use std::io::Write as _;
#[cfg(not(test))]
use std::process::{Command, Stdio};
#[cfg(not(test))]
use std::time::{Duration, Instant};

/// Process a key event and mutate the App state accordingly.
pub fn handle_key(app: &mut App, key: KeyEvent) {
    match app.input_mode {
        InputMode::Normal => handle_normal_mode(app, key),
        InputMode::Search => handle_search_mode(app, key),
    }
}

fn handle_normal_mode(app: &mut App, key: KeyEvent) {
    if app.active_tab == Tab::Settings && handle_settings_mode(app, key) {
        return;
    }
    if app.active_tab == Tab::Platform && handle_platform_details_mode(app, key) {
        return;
    }
    if app.active_tab == Tab::Platform {
        match key.code {
            KeyCode::Left => {
                app.prev_platform_subtab();
                return;
            }
            KeyCode::Right => {
                app.next_platform_subtab();
                return;
            }
            KeyCode::Char('g') => {
                app.set_platform_subtab(PlatformSubtab::GitHub);
                return;
            }
            KeyCode::Char('w') => {
                app.set_platform_subtab(PlatformSubtab::Railway);
                return;
            }
            KeyCode::Char('v') => {
                app.set_platform_subtab(PlatformSubtab::Vercel);
                return;
            }
            KeyCode::Char('d') | KeyCode::Enter => {
                if app.platform_subtab == PlatformSubtab::GitHub {
                    app.toggle_platform_logs_panel();
                }
                return;
            }
            _ => {}
        }
    }

    match key.code {
        // Quit
        KeyCode::Char('q') => app.should_quit = true,
        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            app.should_quit = true;
        }

        // Row navigation
        KeyCode::Char('j') | KeyCode::Down => {
            let count = app.row_count();
            if count > 0 && app.selected_row < count - 1 {
                app.selected_row += 1;
            }
        }
        KeyCode::Char('k') | KeyCode::Up => {
            if app.selected_row > 0 {
                app.selected_row -= 1;
            }
        }

        // Tab switching — number keys
        KeyCode::Char('1') => {
            app.active_tab = Tab::Unified;
            app.selected_row = 0;
            app.close_platform_details_mode();
            app.clamp_selection();
        }
        KeyCode::Char('2') => {
            app.active_tab = Tab::Platform;
            app.selected_row = 0;
            app.clamp_selection();
        }
        KeyCode::Char('3') => {
            app.active_tab = Tab::Health;
            app.selected_row = 0;
            app.close_platform_details_mode();
            app.clamp_selection();
        }
        KeyCode::Char('4') => {
            app.active_tab = Tab::Settings;
            app.selected_row = 0;
            app.close_platform_details_mode();
            app.clamp_selection();
        }
        KeyCode::Char('5') => {
            app.active_tab = Tab::Logs;
            app.selected_row = 0;
            app.close_platform_details_mode();
            app.clamp_selection();
        }

        // Tab cycling
        KeyCode::Tab => {
            app.active_tab = app.active_tab.next();
            app.selected_row = 0;
            if app.active_tab != Tab::Platform {
                app.close_platform_details_mode();
            }
            app.clamp_selection();
        }
        KeyCode::BackTab => {
            app.active_tab = app.active_tab.prev();
            app.selected_row = 0;
            if app.active_tab != Tab::Platform {
                app.close_platform_details_mode();
            }
            app.clamp_selection();
        }

        // Log filter cycling
        KeyCode::Char('f') if app.active_tab == Tab::Logs => {
            app.log_filter = app.log_filter.next();
            app.selected_row = 0;
        }
        KeyCode::Char('c')
            if app.active_tab == Tab::Logs
                && !key.modifiers.contains(KeyModifiers::CONTROL)
                && !key.modifiers.contains(KeyModifiers::ALT) =>
        {
            if let Some(entry) = app.selected_log_entry() {
                let result = copy_text_to_clipboard(&entry.message);
                let (level, message) = match result {
                    Ok(()) => (
                        tracing::Level::INFO,
                        "copied selected log message to clipboard".to_string(),
                    ),
                    Err(err) => (tracing::Level::WARN, format!("copy failed: {err}")),
                };
                app.log_buffer.push(LogEntry {
                    timestamp: Utc::now(),
                    level,
                    target: "pulsos_cli::tui::keys".to_string(),
                    message,
                });
            } else {
                app.log_buffer.push(LogEntry {
                    timestamp: Utc::now(),
                    level: tracing::Level::WARN,
                    target: "pulsos_cli::tui::keys".to_string(),
                    message: "copy failed: no log entry selected".to_string(),
                });
            }
        }

        // Unified tab sort cycling
        KeyCode::Char('s') if app.active_tab == Tab::Unified => {
            app.unified_sort = app.unified_sort.next();
            app.selected_row = 0;
        }

        // Force refresh
        KeyCode::Char('r') => {
            app.force_refresh = true;
        }

        // Enter search mode
        KeyCode::Char('/') => {
            app.input_mode = InputMode::Search;
            app.search_query.clear();
        }

        _ => {}
    }
}

fn handle_platform_details_mode(app: &mut App, key: KeyEvent) -> bool {
    if !app.platform_details_active() {
        return false;
    }

    match key.code {
        KeyCode::Esc => {
            app.close_platform_details_mode();
            true
        }
        KeyCode::Right => {
            app.details_toggle_or_open_right();
            true
        }
        KeyCode::Left => {
            app.details_left_action();
            true
        }
        KeyCode::Enter => {
            app.details_toggle_or_open_right();
            true
        }
        KeyCode::Char('d') if !key.modifiers.contains(KeyModifiers::CONTROL) => {
            app.toggle_platform_logs_panel();
            true
        }
        KeyCode::Char('j') | KeyCode::Down => {
            if matches!(app.details_current_focus(), Some(DetailsFocus::RightPanel)) {
                app.details_scroll_right(1);
            } else {
                app.details_move_tree_cursor(1);
            }
            true
        }
        KeyCode::Char('k') | KeyCode::Up => {
            if matches!(app.details_current_focus(), Some(DetailsFocus::RightPanel)) {
                app.details_scroll_right(-1);
            } else {
                app.details_move_tree_cursor(-1);
            }
            true
        }
        KeyCode::PageDown => {
            app.details_scroll_right(20);
            app.details_focus_right();
            true
        }
        KeyCode::PageUp => {
            app.details_scroll_right(-20);
            app.details_focus_right();
            true
        }
        KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            app.details_scroll_right(10);
            app.details_focus_right();
            true
        }
        KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            app.details_scroll_right(-10);
            app.details_focus_right();
            true
        }
        _ => false,
    }
}

fn handle_settings_mode(app: &mut App, key: KeyEvent) -> bool {
    match app.settings_flow {
        SettingsFlowState::TokenEntry => {
            match key.code {
                KeyCode::Esc => {
                    app.settings_flow = SettingsFlowState::Idle;
                    app.token_input.clear();
                }
                KeyCode::Enter => {
                    let platform = app.selected_settings_platform();
                    let token = app.token_input.trim().to_string();
                    if token.is_empty() {
                        app.settings_message = Some("token cannot be empty".to_string());
                    } else {
                        app.queue_action(
                            ActionRequest::ValidateAndStoreToken { platform, token },
                            SettingsFlowState::ValidatingToken,
                        );
                    }
                }
                KeyCode::Backspace => {
                    app.token_input.pop();
                }
                KeyCode::Char(c)
                    if !key.modifiers.contains(KeyModifiers::CONTROL)
                        && !key.modifiers.contains(KeyModifiers::ALT) =>
                {
                    app.token_input.push(c);
                }
                _ => {}
            }
            return true;
        }
        SettingsFlowState::ProviderActions => {
            match key.code {
                KeyCode::Esc => {
                    app.settings_flow = SettingsFlowState::Idle;
                    app.onboarding.reset();
                }
                KeyCode::Char('j') | KeyCode::Down => {
                    app.onboarding.platform_cursor = (app.onboarding.platform_cursor + 1).min(2);
                }
                KeyCode::Char('k') | KeyCode::Up => {
                    if app.onboarding.platform_cursor > 0 {
                        app.onboarding.platform_cursor -= 1;
                    }
                }
                KeyCode::Char(' ') => {
                    let idx = app.onboarding.platform_cursor.min(2);
                    app.onboarding.platform_selected[idx] = !app.onboarding.platform_selected[idx];
                }
                KeyCode::Enter => {
                    let platforms = app.onboarding.selected_platforms();
                    if platforms.is_empty() {
                        app.settings_message = Some("select at least one provider".to_string());
                    } else {
                        app.queue_action(
                            ActionRequest::Discover { platforms },
                            SettingsFlowState::DiscoveryScanning,
                        );
                    }
                }
                _ => {}
            }
            return true;
        }
        SettingsFlowState::ResourceSelection => {
            match key.code {
                KeyCode::Esc => {
                    app.settings_flow = SettingsFlowState::Idle;
                    app.onboarding.reset();
                }
                KeyCode::Char('j') | KeyCode::Down => {
                    app.onboarding.resource_cursor =
                        app.onboarding.resource_cursor.saturating_add(1);
                    app.onboarding.clamp_resource_cursor();
                }
                KeyCode::Char('k') | KeyCode::Up => {
                    app.onboarding.resource_cursor =
                        app.onboarding.resource_cursor.saturating_sub(1);
                }
                KeyCode::Char(' ') => {
                    app.onboarding
                        .toggle_resource(app.onboarding.resource_cursor);
                }
                KeyCode::Enter => {
                    if app.onboarding.selected_count() == 0 {
                        app.settings_message = Some("select at least one resource".to_string());
                    } else {
                        app.queue_action(
                            ActionRequest::BuildCorrelationPreview {
                                discovery: app.onboarding.selected_discovery(),
                            },
                            SettingsFlowState::Applying,
                        );
                    }
                }
                _ => {}
            }
            return true;
        }
        SettingsFlowState::CorrelationReview => {
            match key.code {
                KeyCode::Esc => app.settings_flow = SettingsFlowState::ResourceSelection,
                KeyCode::Enter => {
                    app.queue_action(
                        ActionRequest::ApplyCorrelations {
                            discovery: app.onboarding.selected_discovery(),
                        },
                        SettingsFlowState::Applying,
                    );
                }
                _ => {}
            }
            return true;
        }
        SettingsFlowState::ValidatingToken
        | SettingsFlowState::DiscoveryScanning
        | SettingsFlowState::Applying => {
            return true;
        }
        SettingsFlowState::ValidationResult | SettingsFlowState::Idle => {}
    }

    match key.code {
        KeyCode::Esc => {
            app.settings_flow = SettingsFlowState::Idle;
            app.onboarding.reset();
            true
        }
        KeyCode::Enter => {
            app.settings_flow = SettingsFlowState::ProviderActions;
            app.onboarding.reset();
            true
        }
        KeyCode::Char('t') => {
            if app.selected_token_from_env() {
                app.settings_message = Some(
                    "env token is active; unset env var or press T to store override".to_string(),
                );
            } else {
                app.settings_flow = SettingsFlowState::TokenEntry;
                app.token_input.clear();
            }
            true
        }
        KeyCode::Char('T') => {
            app.settings_flow = SettingsFlowState::TokenEntry;
            app.token_input.clear();
            if app.selected_token_from_env() {
                app.settings_message = Some(
                    "stored override can be saved; env token stays active until unset".to_string(),
                );
            }
            true
        }
        KeyCode::Char('x') => {
            if app.selected_token_from_env() {
                app.settings_message =
                    Some("cannot remove env token; unset it in shell".to_string());
                return true;
            }
            let platform = app.selected_settings_platform();
            app.queue_action(
                ActionRequest::RemoveToken { platform },
                SettingsFlowState::Applying,
            );
            true
        }
        KeyCode::Char('v') => {
            let platform = app.selected_settings_platform();
            app.queue_action(
                ActionRequest::ValidatePlatform { platform },
                SettingsFlowState::Applying,
            );
            true
        }
        KeyCode::Char('o') => {
            app.settings_flow = SettingsFlowState::ProviderActions;
            app.onboarding.reset();
            true
        }
        _ => false,
    }
}

fn handle_search_mode(app: &mut App, key: KeyEvent) {
    match key.code {
        // Exit search mode
        KeyCode::Esc => {
            app.input_mode = InputMode::Normal;
            app.search_query.clear();
        }

        // Apply search and return to normal mode
        KeyCode::Enter => {
            app.input_mode = InputMode::Normal;
            // search_query remains set — render will use it as filter
        }

        // Delete last char
        KeyCode::Backspace => {
            app.search_query.pop();
        }

        // Type character (ignore Ctrl/Alt-modified keys)
        KeyCode::Char(c)
            if !key.modifiers.contains(KeyModifiers::CONTROL)
                && !key.modifiers.contains(KeyModifiers::ALT) =>
        {
            app.search_query.push(c);
        }

        _ => {}
    }
}

#[cfg(test)]
fn copy_text_to_clipboard(_text: &str) -> Result<(), String> {
    Ok(())
}

#[cfg(not(test))]
fn copy_text_to_clipboard(text: &str) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        return copy_via_command("pbcopy", &[], text);
    }

    #[cfg(target_os = "windows")]
    {
        return copy_via_command("clip", &[], text);
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        copy_via_command("wl-copy", &[], text)
            .or_else(|_| copy_via_command("xclip", &["-selection", "clipboard"], text))
    }
}

#[cfg(not(test))]
fn copy_via_command(cmd: &str, args: &[&str], text: &str) -> Result<(), String> {
    let mut child = Command::new(cmd)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("unable to start {cmd}: {e}"))?;
    if let Some(stdin) = child.stdin.as_mut() {
        stdin
            .write_all(text.as_bytes())
            .map_err(|e| format!("unable to write to {cmd}: {e}"))?;
    }
    // Close stdin before waiting and bound wait time so TUI cannot freeze.
    drop(child.stdin.take());
    let deadline = Instant::now() + Duration::from_secs(2);
    let output = loop {
        match child.try_wait() {
            Ok(Some(_status)) => {
                break child
                    .wait_with_output()
                    .map_err(|e| format!("{cmd} failed: {e}"))?;
            }
            Ok(None) => {
                if Instant::now() >= deadline {
                    let _ = child.kill();
                    return Err(format!("{cmd} timed out"));
                }
                std::thread::sleep(Duration::from_millis(25));
            }
            Err(e) => return Err(format!("{cmd} failed: {e}")),
        }
    };
    if output.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        if stderr.is_empty() {
            Err(format!("{cmd} exited with status {}", output.status))
        } else {
            Err(format!("{cmd}: {stderr}"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::actions::ActionRequest;
    use crate::tui::app::{DataSnapshot, InputMode, LogFilter, PlatformSubtab, Tab};
    use crate::tui::log_buffer::{LogEntry, LogRingBuffer};
    use crate::tui::settings_flow::SettingsFlowState;
    use chrono::Utc;
    use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};
    use pulsos_core::auth::PlatformKind;
    use pulsos_core::config::types::TuiConfig;
    use pulsos_core::domain::deployment::{
        DeploymentEvent, DeploymentStatus, EventMetadata, JobDetail, JobStepSummary, Platform,
    };
    use pulsos_core::health::{PlatformHealthDetails, PlatformHealthReport, PlatformHealthState};

    fn make_key(code: KeyCode) -> KeyEvent {
        KeyEvent {
            code,
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        }
    }

    fn make_key_with_mod(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
        KeyEvent {
            code,
            modifiers,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        }
    }

    fn test_app() -> App {
        // Simulate some rows so navigation works
        let data = DataSnapshot {
            health_scores: vec![
                ("proj-a".into(), 90),
                ("proj-b".into(), 50),
                ("proj-c".into(), 10),
            ],
            ..Default::default()
        };
        App::new(data, TuiConfig::default(), LogRingBuffer::new())
    }

    fn platform_app_with_event(platform: Platform) -> App {
        let data = DataSnapshot {
            events: vec![DeploymentEvent {
                id: "evt-1".into(),
                platform,
                status: DeploymentStatus::Success,
                commit_sha: Some("abc123".into()),
                branch: Some("main".into()),
                title: Some("CI".into()),
                actor: Some("vivallo".into()),
                created_at: Utc::now(),
                updated_at: None,
                duration_secs: Some(10),
                url: None,
                metadata: EventMetadata::default(),
                is_from_cache: false,
            }],
            ..Default::default()
        };
        let mut app = App::new(data, TuiConfig::default(), LogRingBuffer::new());
        app.active_tab = Tab::Platform;
        app
    }

    fn platform_app_with_github_tree() -> App {
        let data = DataSnapshot {
            events: vec![DeploymentEvent {
                id: "100001".into(),
                platform: Platform::GitHub,
                status: DeploymentStatus::Success,
                commit_sha: Some("abc123".into()),
                branch: Some("main".into()),
                title: Some("CI".into()),
                actor: Some("vivallo".into()),
                created_at: Utc::now(),
                updated_at: None,
                duration_secs: Some(10),
                url: None,
                metadata: EventMetadata {
                    source_id: Some("myorg/my-saas".into()),
                    job_details: vec![JobDetail {
                        job_id: Some(700001),
                        name: "build".into(),
                        status: DeploymentStatus::Success,
                        html_url: Some(
                            "https://github.com/myorg/my-saas/actions/runs/100001/job/700001"
                                .into(),
                        ),
                        steps: vec![JobStepSummary {
                            number: 1,
                            name: "Checkout".into(),
                            status: DeploymentStatus::Success,
                            duration_secs: Some(2),
                            started_at: Some(Utc::now()),
                            completed_at: Some(Utc::now()),
                        }],
                    }],
                    ..Default::default()
                },
                is_from_cache: false,
            }],
            ..Default::default()
        };
        let mut app = App::new(data, TuiConfig::default(), LogRingBuffer::new());
        app.active_tab = Tab::Platform;
        app
    }

    fn settings_app() -> App {
        let data = DataSnapshot {
            platform_health: vec![PlatformHealthReport {
                platform: PlatformKind::GitHub,
                state: PlatformHealthState::NoToken,
                reason: "no token".into(),
                next_action: "set token".into(),
                token_source: None,
                last_checked_at: Utc::now(),
                details: PlatformHealthDetails::None,
            }],
            ..Default::default()
        };
        let mut app = App::new(data, TuiConfig::default(), LogRingBuffer::new());
        app.active_tab = Tab::Settings;
        app
    }

    fn env_settings_app() -> App {
        let data = DataSnapshot {
            platform_health: vec![PlatformHealthReport {
                platform: PlatformKind::GitHub,
                state: PlatformHealthState::Ready,
                reason: "env token".into(),
                next_action: "none".into(),
                token_source: Some("GITHUB_TOKEN".into()),
                last_checked_at: Utc::now(),
                details: PlatformHealthDetails::None,
            }],
            ..Default::default()
        };
        let mut app = App::new(data, TuiConfig::default(), LogRingBuffer::new());
        app.active_tab = Tab::Settings;
        app
    }

    #[test]
    fn quit_on_q() {
        let mut app = test_app();
        handle_key(&mut app, make_key(KeyCode::Char('q')));
        assert!(app.should_quit);
    }

    #[test]
    fn quit_on_ctrl_c() {
        let mut app = test_app();
        handle_key(
            &mut app,
            make_key_with_mod(KeyCode::Char('c'), KeyModifiers::CONTROL),
        );
        assert!(app.should_quit);
    }

    #[test]
    fn navigate_down_j() {
        let mut app = test_app();
        app.active_tab = Tab::Health; // has 3 rows
        assert_eq!(app.selected_row, 0);
        handle_key(&mut app, make_key(KeyCode::Char('j')));
        assert_eq!(app.selected_row, 1);
        handle_key(&mut app, make_key(KeyCode::Char('j')));
        assert_eq!(app.selected_row, 2);
        // At last row — should not exceed
        handle_key(&mut app, make_key(KeyCode::Char('j')));
        assert_eq!(app.selected_row, 2);
    }

    #[test]
    fn navigate_up_k() {
        let mut app = test_app();
        app.active_tab = Tab::Health;
        app.selected_row = 2;
        handle_key(&mut app, make_key(KeyCode::Char('k')));
        assert_eq!(app.selected_row, 1);
        handle_key(&mut app, make_key(KeyCode::Char('k')));
        assert_eq!(app.selected_row, 0);
        // At first row — should not go negative
        handle_key(&mut app, make_key(KeyCode::Char('k')));
        assert_eq!(app.selected_row, 0);
    }

    #[test]
    fn navigate_with_arrow_keys() {
        let mut app = test_app();
        app.active_tab = Tab::Health;
        handle_key(&mut app, make_key(KeyCode::Down));
        assert_eq!(app.selected_row, 1);
        handle_key(&mut app, make_key(KeyCode::Up));
        assert_eq!(app.selected_row, 0);
    }

    #[test]
    fn switch_tabs_with_numbers() {
        let mut app = test_app();
        assert_eq!(app.active_tab, Tab::Unified);

        handle_key(&mut app, make_key(KeyCode::Char('2')));
        assert_eq!(app.active_tab, Tab::Platform);

        handle_key(&mut app, make_key(KeyCode::Char('3')));
        assert_eq!(app.active_tab, Tab::Health);

        handle_key(&mut app, make_key(KeyCode::Char('4')));
        assert_eq!(app.active_tab, Tab::Settings);

        handle_key(&mut app, make_key(KeyCode::Char('1')));
        assert_eq!(app.active_tab, Tab::Unified);
    }

    #[test]
    fn switch_tabs_resets_selection() {
        let mut app = test_app();
        app.active_tab = Tab::Health;
        app.selected_row = 2;
        handle_key(&mut app, make_key(KeyCode::Char('1')));
        assert_eq!(app.selected_row, 0);
    }

    #[test]
    fn cycle_tabs_with_tab_key() {
        let mut app = test_app();
        assert_eq!(app.active_tab, Tab::Unified);
        handle_key(&mut app, make_key(KeyCode::Tab));
        assert_eq!(app.active_tab, Tab::Platform);
        handle_key(&mut app, make_key(KeyCode::Tab));
        assert_eq!(app.active_tab, Tab::Health);
        handle_key(&mut app, make_key(KeyCode::Tab));
        assert_eq!(app.active_tab, Tab::Settings);
        handle_key(&mut app, make_key(KeyCode::Tab));
        assert_eq!(app.active_tab, Tab::Logs);
        handle_key(&mut app, make_key(KeyCode::Tab));
        assert_eq!(app.active_tab, Tab::Unified);
    }

    #[test]
    fn cycle_tabs_backward_with_backtab() {
        let mut app = test_app();
        assert_eq!(app.active_tab, Tab::Unified);
        handle_key(&mut app, make_key(KeyCode::BackTab));
        assert_eq!(app.active_tab, Tab::Logs);
        handle_key(&mut app, make_key(KeyCode::BackTab));
        assert_eq!(app.active_tab, Tab::Settings);
        handle_key(&mut app, make_key(KeyCode::BackTab));
        assert_eq!(app.active_tab, Tab::Health);
        handle_key(&mut app, make_key(KeyCode::BackTab));
        assert_eq!(app.active_tab, Tab::Platform);
    }

    #[test]
    fn switch_to_logs_with_key_5() {
        let mut app = test_app();
        handle_key(&mut app, make_key(KeyCode::Char('5')));
        assert_eq!(app.active_tab, Tab::Logs);
        assert_eq!(app.selected_row, 0);
    }

    #[test]
    fn force_refresh() {
        let mut app = test_app();
        assert!(!app.force_refresh);
        handle_key(&mut app, make_key(KeyCode::Char('r')));
        assert!(app.force_refresh);
    }

    #[test]
    fn enter_search_mode() {
        let mut app = test_app();
        assert_eq!(app.input_mode, InputMode::Normal);
        handle_key(&mut app, make_key(KeyCode::Char('/')));
        assert_eq!(app.input_mode, InputMode::Search);
        assert!(app.search_query.is_empty());
    }

    #[test]
    fn search_mode_typing() {
        let mut app = test_app();
        app.input_mode = InputMode::Search;

        handle_key(&mut app, make_key(KeyCode::Char('m')));
        handle_key(&mut app, make_key(KeyCode::Char('y')));
        assert_eq!(app.search_query, "my");

        handle_key(&mut app, make_key(KeyCode::Backspace));
        assert_eq!(app.search_query, "m");
    }

    #[test]
    fn search_mode_esc_clears_and_exits() {
        let mut app = test_app();
        app.input_mode = InputMode::Search;
        app.search_query = "test".into();

        handle_key(&mut app, make_key(KeyCode::Esc));
        assert_eq!(app.input_mode, InputMode::Normal);
        assert!(app.search_query.is_empty());
    }

    #[test]
    fn search_mode_enter_applies_and_exits() {
        let mut app = test_app();
        app.input_mode = InputMode::Search;
        app.search_query = "prod".into();

        handle_key(&mut app, make_key(KeyCode::Enter));
        assert_eq!(app.input_mode, InputMode::Normal);
        assert_eq!(app.search_query, "prod"); // kept for filtering
    }

    #[test]
    fn settings_token_entry_queues_store_action() {
        let mut app = settings_app();
        handle_key(&mut app, make_key(KeyCode::Char('t')));
        assert_eq!(app.settings_flow, SettingsFlowState::TokenEntry);

        handle_key(&mut app, make_key(KeyCode::Char('a')));
        handle_key(&mut app, make_key(KeyCode::Char('b')));
        handle_key(&mut app, make_key(KeyCode::Char('c')));
        handle_key(&mut app, make_key(KeyCode::Enter));

        assert_eq!(app.settings_flow, SettingsFlowState::ValidatingToken);
        match app.pending_action.take() {
            Some(ActionRequest::ValidateAndStoreToken { platform, token }) => {
                assert_eq!(platform, PlatformKind::GitHub);
                assert_eq!(token, "abc");
            }
            other => panic!("unexpected action queued: {other:?}"),
        }
    }

    #[test]
    fn settings_onboarding_discovery_requires_selection() {
        let mut app = settings_app();
        handle_key(&mut app, make_key(KeyCode::Char('o')));
        assert_eq!(app.settings_flow, SettingsFlowState::ProviderActions);

        handle_key(&mut app, make_key(KeyCode::Enter));
        assert!(app.pending_action.is_none());

        handle_key(&mut app, make_key(KeyCode::Char(' ')));
        handle_key(&mut app, make_key(KeyCode::Enter));
        match app.pending_action.take() {
            Some(ActionRequest::Discover { platforms }) => {
                assert_eq!(platforms, vec![PlatformKind::GitHub]);
            }
            other => panic!("unexpected action queued: {other:?}"),
        }
    }

    #[test]
    fn settings_messages_use_normalized_validation_copy() {
        let mut app = settings_app();
        handle_key(&mut app, make_key(KeyCode::Char('t')));
        handle_key(&mut app, make_key(KeyCode::Enter));
        assert_eq!(
            app.settings_message.as_deref(),
            Some("token cannot be empty")
        );

        app.settings_flow = SettingsFlowState::ProviderActions;
        app.onboarding.reset();
        handle_key(&mut app, make_key(KeyCode::Enter));
        assert_eq!(
            app.settings_message.as_deref(),
            Some("select at least one provider")
        );

        app.settings_flow = SettingsFlowState::ResourceSelection;
        app.onboarding.reset();
        handle_key(&mut app, make_key(KeyCode::Enter));
        assert_eq!(
            app.settings_message.as_deref(),
            Some("select at least one resource")
        );
    }

    #[test]
    fn settings_messages_explain_env_token_limits() {
        let mut app = env_settings_app();
        handle_key(&mut app, make_key(KeyCode::Char('t')));
        assert_eq!(
            app.settings_message.as_deref(),
            Some("env token is active; unset env var or press T to store override")
        );

        handle_key(&mut app, make_key(KeyCode::Char('x')));
        assert_eq!(
            app.settings_message.as_deref(),
            Some("cannot remove env token; unset it in shell")
        );
    }

    fn logs_app() -> App {
        let log_buffer = LogRingBuffer::new();
        log_buffer.push(LogEntry {
            timestamp: Utc::now(),
            level: tracing::Level::ERROR,
            target: String::new(),
            message: "error msg".into(),
        });
        log_buffer.push(LogEntry {
            timestamp: Utc::now(),
            level: tracing::Level::WARN,
            target: String::new(),
            message: "warn msg".into(),
        });
        log_buffer.push(LogEntry {
            timestamp: Utc::now(),
            level: tracing::Level::INFO,
            target: String::new(),
            message: "info msg".into(),
        });
        let mut app = App::new(DataSnapshot::default(), TuiConfig::default(), log_buffer);
        app.active_tab = Tab::Logs;
        app
    }

    #[test]
    fn logs_filter_cycles_with_f() {
        let mut app = logs_app();
        assert_eq!(app.log_filter, LogFilter::All);

        handle_key(&mut app, make_key(KeyCode::Char('f')));
        assert_eq!(app.log_filter, LogFilter::Error);
        assert_eq!(app.selected_row, 0);

        handle_key(&mut app, make_key(KeyCode::Char('f')));
        assert_eq!(app.log_filter, LogFilter::Warn);

        handle_key(&mut app, make_key(KeyCode::Char('f')));
        assert_eq!(app.log_filter, LogFilter::Info);

        handle_key(&mut app, make_key(KeyCode::Char('f')));
        assert_eq!(app.log_filter, LogFilter::All);
    }

    #[test]
    fn f_key_does_nothing_on_other_tabs() {
        let mut app = test_app();
        app.active_tab = Tab::Unified;
        let filter_before = app.log_filter;
        handle_key(&mut app, make_key(KeyCode::Char('f')));
        assert_eq!(app.log_filter, filter_before);

        app.active_tab = Tab::Health;
        handle_key(&mut app, make_key(KeyCode::Char('f')));
        assert_eq!(app.log_filter, filter_before);
    }

    #[test]
    fn c_copies_selected_log_message_on_logs_tab() {
        let mut app = logs_app();
        let before_len = app.log_buffer.len();
        app.selected_row = 0;
        handle_key(&mut app, make_key(KeyCode::Char('c')));
        let latest = app.log_buffer.latest().expect("latest log entry");
        assert!(latest.message.contains("copied selected log message"));
        assert_eq!(latest.level, tracing::Level::INFO);
        assert_eq!(app.log_buffer.len(), before_len + 1);
    }

    #[test]
    fn c_does_nothing_on_non_logs_tabs() {
        let mut app = test_app();
        let before_len = app.log_buffer.len();
        app.active_tab = Tab::Unified;
        handle_key(&mut app, make_key(KeyCode::Char('c')));
        assert_eq!(app.log_buffer.len(), before_len);
    }

    #[test]
    fn s_key_cycles_unified_sort() {
        use crate::tui::app::UnifiedSort;
        let mut app = test_app();
        app.active_tab = Tab::Unified;

        assert_eq!(app.unified_sort, UnifiedSort::ByTime);
        handle_key(&mut app, make_key(KeyCode::Char('s')));
        assert_eq!(app.unified_sort, UnifiedSort::ByPlatform);
        assert_eq!(app.selected_row, 0); // resets selection

        handle_key(&mut app, make_key(KeyCode::Char('s')));
        assert_eq!(app.unified_sort, UnifiedSort::ByTime);
    }

    #[test]
    fn s_key_does_nothing_on_other_tabs() {
        use crate::tui::app::UnifiedSort;
        let mut app = test_app();

        app.active_tab = Tab::Platform;
        handle_key(&mut app, make_key(KeyCode::Char('s')));
        assert_eq!(app.unified_sort, UnifiedSort::ByTime); // unchanged

        app.active_tab = Tab::Health;
        handle_key(&mut app, make_key(KeyCode::Char('s')));
        assert_eq!(app.unified_sort, UnifiedSort::ByTime); // unchanged

        app.active_tab = Tab::Logs;
        handle_key(&mut app, make_key(KeyCode::Char('s')));
        assert_eq!(app.unified_sort, UnifiedSort::ByTime); // unchanged
    }

    #[test]
    fn d_toggles_github_details_on_platform_tab() {
        let mut app = platform_app_with_event(Platform::GitHub);
        app.platform_subtab = PlatformSubtab::GitHub;
        assert!(app.platform_details_mode.is_none());

        handle_key(&mut app, make_key(KeyCode::Char('d')));
        assert!(app.platform_details_active());
        handle_key(&mut app, make_key(KeyCode::Char('d')));
        assert!(!app.platform_details_active());
    }

    #[test]
    fn enter_opens_github_details_panel() {
        let mut app = platform_app_with_event(Platform::GitHub);
        app.platform_subtab = PlatformSubtab::GitHub;

        handle_key(&mut app, make_key(KeyCode::Enter));
        assert!(app.platform_details_active());
    }

    #[test]
    fn enter_keeps_details_closed_when_not_toggled() {
        let mut app = platform_app_with_event(Platform::GitHub);
        app.set_platform_subtab(PlatformSubtab::Railway);

        handle_key(&mut app, make_key(KeyCode::Enter));
        assert!(app.platform_details_mode.is_none());
    }

    #[test]
    fn platform_left_right_switches_subtabs_when_details_closed() {
        let mut app = platform_app_with_event(Platform::GitHub);
        app.platform_subtab = PlatformSubtab::GitHub;

        handle_key(&mut app, make_key(KeyCode::Right));
        assert_eq!(app.platform_subtab, PlatformSubtab::Railway);
        handle_key(&mut app, make_key(KeyCode::Right));
        assert_eq!(app.platform_subtab, PlatformSubtab::Vercel);
        handle_key(&mut app, make_key(KeyCode::Left));
        assert_eq!(app.platform_subtab, PlatformSubtab::Railway);
    }

    #[test]
    fn platform_gwv_shortcuts_select_subtabs() {
        let mut app = platform_app_with_event(Platform::GitHub);
        app.platform_subtab = PlatformSubtab::Railway;

        handle_key(&mut app, make_key(KeyCode::Char('v')));
        assert_eq!(app.platform_subtab, PlatformSubtab::Vercel);
        handle_key(&mut app, make_key(KeyCode::Char('g')));
        assert_eq!(app.platform_subtab, PlatformSubtab::GitHub);
        handle_key(&mut app, make_key(KeyCode::Char('w')));
        assert_eq!(app.platform_subtab, PlatformSubtab::Railway);
    }

    #[test]
    fn switching_platform_subtab_closes_details_and_resets_selection() {
        let mut app = platform_app_with_github_tree();
        app.selected_row = 3;
        handle_key(&mut app, make_key(KeyCode::Char('d')));
        assert!(app.platform_details_mode.is_some());

        handle_key(&mut app, make_key(KeyCode::Esc));
        assert!(app.platform_details_mode.is_none());
        handle_key(&mut app, make_key(KeyCode::Right));
        assert_eq!(app.platform_subtab, PlatformSubtab::Railway);
        assert_eq!(app.selected_row, 0);
    }

    #[test]
    fn right_expands_job_then_left_collapses() {
        let mut app = platform_app_with_github_tree();
        handle_key(&mut app, make_key(KeyCode::Char('d'))); // open dropdown

        // Jobs start collapsed.
        let expanded = app
            .platform_details_mode
            .as_ref()
            .map(|d| d.expanded_jobs.contains(&700001))
            .unwrap_or(false);
        assert!(!expanded, "Job should start collapsed");

        // Move to first job and press Right → expands it, stays in LeftTree.
        handle_key(&mut app, make_key(KeyCode::Down));
        handle_key(&mut app, make_key(KeyCode::Right));
        let expanded = app
            .platform_details_mode
            .as_ref()
            .map(|d| d.expanded_jobs.contains(&700001))
            .unwrap_or(false);
        assert!(expanded, "Right on collapsed job should expand it");

        // Left on expanded job in LeftTree → collapse.
        handle_key(&mut app, make_key(KeyCode::Left));
        let expanded = app
            .platform_details_mode
            .as_ref()
            .map(|d| d.expanded_jobs.contains(&700001))
            .unwrap_or(false);
        assert!(
            !expanded,
            "Left on LeftTree with expanded job should collapse it"
        );
    }

    #[test]
    fn esc_closes_platform_details_mode() {
        let mut app = platform_app_with_event(Platform::GitHub);
        handle_key(&mut app, make_key(KeyCode::Char('d')));
        assert!(app.platform_details_mode.is_some());

        handle_key(&mut app, make_key(KeyCode::Esc));
        assert!(app.platform_details_mode.is_none());
    }
}
