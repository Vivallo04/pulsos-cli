//! Visual verification tests for all 6 TUI design guide gaps (Batches 5-7).
//!
//! These tests render widgets to a TestBackend buffer and verify that the
//! expected visual elements appear in the output.

#[cfg(test)]
mod tests {
    use crate::tui::app::{App, DataSnapshot, InputMode, Tab};
    use crate::tui::log_buffer::{LogEntry, LogRingBuffer};
    use crate::tui::theme::Theme;
    use crate::tui::widgets;
    use chrono::Utc;
    use pulsos_core::config::types::TuiConfig;
    use pulsos_core::domain::deployment::*;
    use pulsos_core::domain::health::HealthBreakdown;
    use pulsos_core::domain::project::*;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;
    use tracing::Level;

    fn buffer_to_string(buf: &ratatui::buffer::Buffer) -> String {
        let mut text = String::new();
        for y in buf.area.top()..buf.area.bottom() {
            for x in buf.area.left()..buf.area.right() {
                text.push_str(buf.cell((x, y)).map(|c| c.symbol()).unwrap_or(" "));
            }
        }
        text
    }

    // === Gap 1: highlight_symbol("▶ ") on all tables ===

    #[test]
    fn highlight_symbol_on_unified_table() {
        let backend = TestBackend::new(140, 20);
        let mut terminal = Terminal::new(backend).unwrap();
        let data = DataSnapshot {
            correlated: vec![CorrelatedEvent {
                project_name: Some("my-saas".into()),
                commit_sha: Some("abc123def".into()),
                github: Some(DeploymentEvent {
                    id: "r1".into(),
                    platform: Platform::GitHub,
                    status: DeploymentStatus::Success,
                    commit_sha: Some("abc123def".into()),
                    branch: Some("main".into()),
                    title: Some("CI".into()),
                    actor: Some("dev".into()),
                    created_at: Utc::now(),
                    updated_at: None,
                    duration_secs: Some(42),
                    url: None,
                    metadata: EventMetadata::default(),
                    is_from_cache: false,
                }),
                railway: None,
                vercel: None,
                confidence: Confidence::High,
                timestamp: Utc::now(),
                is_stale: false,
            }],
            ..Default::default()
        };
        let app = App::new(data, TuiConfig::default(), LogRingBuffer::new());
        let theme = Theme::dark();
        terminal
            .draw(|frame| widgets::unified::draw(frame, frame.area(), &app, &theme))
            .unwrap();
        let text = buffer_to_string(terminal.backend().buffer());
        assert!(
            text.contains("▶"),
            "Unified table should show highlight_symbol ▶"
        );
    }

    #[test]
    fn highlight_symbol_on_platform_table() {
        let backend = TestBackend::new(140, 20);
        let mut terminal = Terminal::new(backend).unwrap();
        let data = DataSnapshot {
            events: vec![DeploymentEvent {
                id: "r1".into(),
                platform: Platform::GitHub,
                status: DeploymentStatus::Success,
                commit_sha: Some("abc".into()),
                branch: Some("main".into()),
                title: Some("CI".into()),
                actor: Some("dev".into()),
                created_at: Utc::now(),
                updated_at: None,
                duration_secs: None,
                url: None,
                metadata: EventMetadata::default(),
                is_from_cache: false,
            }],
            ..Default::default()
        };
        let app = App::new(data, TuiConfig::default(), LogRingBuffer::new());
        let theme = Theme::dark();
        terminal
            .draw(|frame| widgets::platform::draw(frame, frame.area(), &app, &theme))
            .unwrap();
        let text = buffer_to_string(terminal.backend().buffer());
        assert!(
            text.contains("▶"),
            "Platform table should show highlight_symbol ▶"
        );
    }

    #[test]
    fn highlight_symbol_on_logs_table() {
        let backend = TestBackend::new(120, 20);
        let mut terminal = Terminal::new(backend).unwrap();
        let log_buffer = LogRingBuffer::new();
        log_buffer.push(LogEntry {
            timestamp: Utc::now(),
            level: Level::WARN,
            target: String::new(),
            message: "test msg".into(),
        });
        let mut app = App::new(DataSnapshot::default(), TuiConfig::default(), log_buffer);
        app.active_tab = Tab::Logs;
        let theme = Theme::dark();
        terminal
            .draw(|frame| widgets::logs::draw(frame, frame.area(), &app, &theme))
            .unwrap();
        let text = buffer_to_string(terminal.backend().buffer());
        assert!(
            text.contains("▶"),
            "Logs table should show highlight_symbol ▶"
        );
    }

    // === Gap 2: Search bar rendered as inline row above table ===

    #[test]
    fn search_bar_inline_on_platform_tab() {
        let backend = TestBackend::new(100, 20);
        let mut terminal = Terminal::new(backend).unwrap();
        let data = DataSnapshot {
            events: vec![DeploymentEvent {
                id: "r1".into(),
                platform: Platform::GitHub,
                status: DeploymentStatus::Success,
                commit_sha: Some("abc".into()),
                branch: Some("main".into()),
                title: Some("production deploy".into()),
                actor: Some("dev".into()),
                created_at: Utc::now(),
                updated_at: None,
                duration_secs: None,
                url: None,
                metadata: EventMetadata::default(),
                is_from_cache: false,
            }],
            ..Default::default()
        };
        let mut app = App::new(data, TuiConfig::default(), LogRingBuffer::new());
        app.input_mode = InputMode::Search;
        app.search_query = "prod".into();
        let theme = Theme::dark();
        terminal
            .draw(|frame| widgets::platform::draw(frame, frame.area(), &app, &theme))
            .unwrap();
        let text = buffer_to_string(terminal.backend().buffer());
        assert!(
            text.contains("/ ") && text.contains("prod"),
            "Search bar should show '/ prod' inline above table"
        );
    }

    #[test]
    fn search_bar_inline_on_unified_tab() {
        let backend = TestBackend::new(140, 20);
        let mut terminal = Terminal::new(backend).unwrap();
        let data = DataSnapshot::default();
        let mut app = App::new(data, TuiConfig::default(), LogRingBuffer::new());
        app.input_mode = InputMode::Search;
        app.search_query = "test".into();
        let theme = Theme::dark();
        terminal
            .draw(|frame| widgets::unified::draw(frame, frame.area(), &app, &theme))
            .unwrap();
        let text = buffer_to_string(terminal.backend().buffer());
        assert!(
            text.contains("/ ") && text.contains("test"),
            "Unified tab search bar should show '/ test'"
        );
    }

    // === Gap 3: 2-row tall health list items ===

    #[test]
    fn health_list_two_row_items_with_status_labels() {
        let backend = TestBackend::new(100, 30);
        let mut terminal = Terminal::new(backend).unwrap();
        let data = DataSnapshot {
            health_scores: vec![
                ("my-saas".into(), 95),
                ("api-server".into(), 75),
                ("frontend".into(), 50),
            ],
            health_breakdowns: vec![],
            health_history: vec![],
            ..Default::default()
        };
        let app = App::new(data, TuiConfig::default(), LogRingBuffer::new());
        let theme = Theme::dark();
        terminal
            .draw(|frame| widgets::health::draw(frame, frame.area(), &app, &theme))
            .unwrap();
        let text = buffer_to_string(terminal.backend().buffer());
        assert!(
            text.contains("Healthy"),
            "Score 95 should show 'Healthy' label"
        );
        assert!(
            text.contains("Degraded"),
            "Score 75 should show 'Degraded' label"
        );
        assert!(
            text.contains("Critical"),
            "Score 50 should show 'Critical' label"
        );
    }

    // === Gap 4: Recent Events section in health detail panel ===

    #[test]
    fn health_detail_shows_recent_events() {
        let backend = TestBackend::new(120, 30);
        let mut terminal = Terminal::new(backend).unwrap();
        let data = DataSnapshot {
            health_scores: vec![("my-saas".into(), 95)],
            health_breakdowns: vec![(
                "my-saas".into(),
                HealthBreakdown {
                    total: 95,
                    github_score: Some(90),
                    railway_score: Some(100),
                    vercel_score: None,
                    github_weight: 50,
                    railway_weight: 50,
                    vercel_weight: 0,
                },
            )],
            health_history: vec![("my-saas".into(), vec![80, 85, 90, 95])],
            correlated: vec![CorrelatedEvent {
                project_name: Some("my-saas".into()),
                commit_sha: Some("abc123".into()),
                github: Some(DeploymentEvent {
                    id: "r1".into(),
                    platform: Platform::GitHub,
                    status: DeploymentStatus::Success,
                    commit_sha: Some("abc123".into()),
                    branch: Some("main".into()),
                    title: Some("Deploy v2.0".into()),
                    actor: Some("dev".into()),
                    created_at: Utc::now(),
                    updated_at: None,
                    duration_secs: None,
                    url: None,
                    metadata: EventMetadata::default(),
                    is_from_cache: false,
                }),
                railway: None,
                vercel: None,
                confidence: Confidence::High,
                timestamp: Utc::now(),
                is_stale: false,
            }],
            ..Default::default()
        };
        let app = App::new(data, TuiConfig::default(), LogRingBuffer::new());
        let theme = Theme::dark();
        terminal
            .draw(|frame| widgets::health::draw(frame, frame.area(), &app, &theme))
            .unwrap();
        let text = buffer_to_string(terminal.backend().buffer());
        assert!(
            text.contains("RECENT EVENTS"),
            "Health detail should show RECENT EVENTS section"
        );
        assert!(
            text.contains("Deploy v2.0"),
            "Recent events should show the correlated event title"
        );
    }

    #[test]
    fn health_detail_shows_no_events_when_empty() {
        let backend = TestBackend::new(120, 30);
        let mut terminal = Terminal::new(backend).unwrap();
        let data = DataSnapshot {
            health_scores: vec![("my-saas".into(), 95)],
            health_breakdowns: vec![],
            health_history: vec![],
            ..Default::default()
        };
        // No correlated events
        let app = App::new(data, TuiConfig::default(), LogRingBuffer::new());
        let theme = Theme::dark();
        terminal
            .draw(|frame| widgets::health::draw(frame, frame.area(), &app, &theme))
            .unwrap();
        let text = buffer_to_string(terminal.backend().buffer());
        assert!(
            text.contains("RECENT EVENTS"),
            "Should still show RECENT EVENTS header"
        );
        assert!(
            text.contains("No events"),
            "Should show 'No events for this project'"
        );
    }

    // === Gap 5: Target column in logs tab ===

    #[test]
    fn logs_tab_shows_target_column() {
        let backend = TestBackend::new(120, 20);
        let mut terminal = Terminal::new(backend).unwrap();
        let log_buffer = LogRingBuffer::new();
        log_buffer.push(LogEntry {
            timestamp: Utc::now(),
            level: Level::WARN,
            target: "pulsos_core::platform::github::client".into(),
            message: "retry attempt 1".into(),
        });
        let mut app = App::new(DataSnapshot::default(), TuiConfig::default(), log_buffer);
        app.active_tab = Tab::Logs;
        let theme = Theme::dark();
        terminal
            .draw(|frame| widgets::logs::draw(frame, frame.area(), &app, &theme))
            .unwrap();
        let text = buffer_to_string(terminal.backend().buffer());
        assert!(
            text.contains("Target"),
            "Logs header should include 'Target' column"
        );
        assert!(
            text.contains("platform::github::client"),
            "Target should be shortened (stripped pulsos_core:: prefix)"
        );
    }

    // === Gap 6: Pipeline stages in platform tab ===

    #[test]
    fn platform_tab_shows_pipeline_stages() {
        let backend = TestBackend::new(160, 20);
        let mut terminal = Terminal::new(backend).unwrap();
        let data = DataSnapshot {
            events: vec![DeploymentEvent {
                id: "run-1".into(),
                platform: Platform::GitHub,
                status: DeploymentStatus::Failed,
                commit_sha: Some("abc123".into()),
                branch: Some("main".into()),
                title: Some("CI".into()),
                actor: Some("dev".into()),
                created_at: Utc::now(),
                updated_at: None,
                duration_secs: Some(120),
                url: None,
                metadata: EventMetadata {
                    workflow_name: Some("CI".into()),
                    trigger_event: Some("push".into()),
                    jobs: vec![
                        JobSummary {
                            name: "Build".into(),
                            status: DeploymentStatus::Success,
                        },
                        JobSummary {
                            name: "Test".into(),
                            status: DeploymentStatus::Success,
                        },
                        JobSummary {
                            name: "Deploy".into(),
                            status: DeploymentStatus::Failed,
                        },
                    ],
                    ..Default::default()
                },
                is_from_cache: false,
            }],
            ..Default::default()
        };
        let app = App::new(data, TuiConfig::default(), LogRingBuffer::new());
        let theme = Theme::dark();
        terminal
            .draw(|frame| widgets::platform::draw(frame, frame.area(), &app, &theme))
            .unwrap();
        let text = buffer_to_string(terminal.backend().buffer());
        assert!(text.contains("Build"), "Pipeline should show 'Build' stage");
        assert!(text.contains("Test"), "Pipeline should show 'Test' stage");
        assert!(
            text.contains("Deploy"),
            "Pipeline should show 'Deploy' stage"
        );
        assert!(
            text.contains("›"),
            "Pipeline stages should be separated by › arrows"
        );
    }

    #[test]
    fn platform_tab_falls_back_to_workflow_detail_without_jobs() {
        let backend = TestBackend::new(160, 20);
        let mut terminal = Terminal::new(backend).unwrap();
        let data = DataSnapshot {
            events: vec![DeploymentEvent {
                id: "run-1".into(),
                platform: Platform::GitHub,
                status: DeploymentStatus::Success,
                commit_sha: Some("abc123".into()),
                branch: Some("main".into()),
                title: Some("CI".into()),
                actor: Some("dev".into()),
                created_at: Utc::now(),
                updated_at: None,
                duration_secs: Some(42),
                url: None,
                metadata: EventMetadata {
                    workflow_name: Some("CI".into()),
                    trigger_event: Some("push".into()),
                    ..Default::default()
                },
                is_from_cache: false,
            }],
            ..Default::default()
        };
        let app = App::new(data, TuiConfig::default(), LogRingBuffer::new());
        let theme = Theme::dark();
        terminal
            .draw(|frame| widgets::platform::draw(frame, frame.area(), &app, &theme))
            .unwrap();
        let text = buffer_to_string(terminal.backend().buffer());
        assert!(
            text.contains("CI (push)"),
            "Without jobs, should show workflow name + trigger"
        );
    }
}
