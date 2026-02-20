//! Keybinding dispatch — maps keyboard input to App mutations.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::actions::ActionRequest;
use super::app::{App, InputMode, Tab};
use super::settings_flow::SettingsFlowState;

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
            app.clamp_selection();
        }
        KeyCode::Char('4') => {
            app.active_tab = Tab::Settings;
            app.selected_row = 0;
            app.clamp_selection();
        }
        KeyCode::Char('5') => {
            app.active_tab = Tab::Logs;
            app.selected_row = 0;
            app.clamp_selection();
        }

        // Tab cycling
        KeyCode::Tab => {
            app.active_tab = app.active_tab.next();
            app.selected_row = 0;
            app.clamp_selection();
        }
        KeyCode::BackTab => {
            app.active_tab = app.active_tab.prev();
            app.selected_row = 0;
            app.clamp_selection();
        }

        // Log filter cycling
        KeyCode::Char('f') if app.active_tab == Tab::Logs => {
            app.log_filter = app.log_filter.next();
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
                    app.onboarding.platform_cursor =
                        (app.onboarding.platform_cursor + 1).min(2);
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
                    app.onboarding.resource_cursor = app.onboarding.resource_cursor.saturating_add(1);
                    app.onboarding.clamp_resource_cursor();
                }
                KeyCode::Char('k') | KeyCode::Up => {
                    app.onboarding.resource_cursor = app.onboarding.resource_cursor.saturating_sub(1);
                }
                KeyCode::Char(' ') => {
                    app.onboarding.toggle_resource(app.onboarding.resource_cursor);
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
                app.settings_message = Some("cannot remove env token; unset it in shell".to_string());
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
mod tests {
    use super::*;
    use crate::tui::actions::ActionRequest;
    use crate::tui::app::{DataSnapshot, InputMode, LogFilter, Tab};
    use crate::tui::log_buffer::{LogEntry, LogRingBuffer};
    use crate::tui::settings_flow::SettingsFlowState;
    use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};
    use chrono::Utc;
    use pulsos_core::auth::PlatformKind;
    use pulsos_core::config::types::TuiConfig;
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
        let mut data = DataSnapshot::default();
        // Simulate some rows so navigation works
        data.health_scores = vec![
            ("proj-a".into(), 90),
            ("proj-b".into(), 50),
            ("proj-c".into(), 10),
        ];
        App::new(data, TuiConfig::default(), LogRingBuffer::new())
    }

    fn settings_app() -> App {
        let mut data = DataSnapshot::default();
        data.platform_health = vec![PlatformHealthReport {
            platform: PlatformKind::GitHub,
            state: PlatformHealthState::NoToken,
            reason: "no token".into(),
            next_action: "set token".into(),
            token_source: None,
            last_checked_at: Utc::now(),
            details: PlatformHealthDetails::None,
        }];
        let mut app = App::new(data, TuiConfig::default(), LogRingBuffer::new());
        app.active_tab = Tab::Settings;
        app
    }

    fn env_settings_app() -> App {
        let mut data = DataSnapshot::default();
        data.platform_health = vec![PlatformHealthReport {
            platform: PlatformKind::GitHub,
            state: PlatformHealthState::Ready,
            reason: "env token".into(),
            next_action: "none".into(),
            token_source: Some("GITHUB_TOKEN".into()),
            last_checked_at: Utc::now(),
            details: PlatformHealthDetails::None,
        }];
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
}
