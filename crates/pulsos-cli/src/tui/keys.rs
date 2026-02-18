//! Keybinding dispatch — maps keyboard input to App mutations.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::app::{App, InputMode, Tab};

/// Process a key event and mutate the App state accordingly.
pub fn handle_key(app: &mut App, key: KeyEvent) {
    match app.input_mode {
        InputMode::Normal => handle_normal_mode(app, key),
        InputMode::Search => handle_search_mode(app, key),
    }
}

fn handle_normal_mode(app: &mut App, key: KeyEvent) {
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

        // Type character
        KeyCode::Char(c) => {
            app.search_query.push(c);
        }

        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::app::{DataSnapshot, InputMode, Tab};
    use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};
    use pulsos_core::config::types::TuiConfig;

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
        App::new(data, TuiConfig::default())
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
        assert_eq!(app.active_tab, Tab::Unified);
    }

    #[test]
    fn cycle_tabs_backward_with_backtab() {
        let mut app = test_app();
        assert_eq!(app.active_tab, Tab::Unified);
        handle_key(&mut app, make_key(KeyCode::BackTab));
        assert_eq!(app.active_tab, Tab::Health);
        handle_key(&mut app, make_key(KeyCode::BackTab));
        assert_eq!(app.active_tab, Tab::Platform);
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
}
