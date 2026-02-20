//! Tab 4: Settings — platform configuration and readiness diagnostics.

use ratatui::{
    layout::{Constraint, Layout, Rect},
    text::Span,
    widgets::{Block, Borders, Cell, Paragraph, Row, Table, TableState, Wrap},
    Frame,
};

use crate::tui::app::App;
use crate::tui::settings_flow::SettingsFlowState;
use crate::tui::theme::Theme;
use pulsos_core::health::{PlatformHealthDetails, PlatformHealthReport, PlatformHealthState};

mod copy {
    pub const PROVIDER: &str = "Provider";
    pub const TOKEN_SOURCE: &str = "Token Source";
    pub const REASON: &str = "Reason";
    pub const NEXT_ACTION: &str = "Next Action";
    pub const ACTIONS: &str = "Actions";
    pub const RESULT: &str = "Result";
    pub const PROVIDER_STATS: &str = "Provider Stats";

    pub const ACTION_TOKEN: &str = "t set/replace token   v validate";
    pub const ACTION_REMOVE: &str = "x remove token        o onboard";
    pub const ACTION_SELECT: &str = "Enter select providers";
    pub const ENV_WARNING: &str = "env token is read-only; press T to store override";

    pub const FLOW_TOKEN_INPUT: &str = "Token Input";
    pub const FLOW_TOKEN_HINT: &str = "enter token, Enter to validate+save";
    pub const FLOW_VALIDATING_TOKEN: &str = "validating token...";
    pub const FLOW_ONBOARD_PROVIDERS: &str = "Onboard: Select providers";
    pub const FLOW_DISCOVERING: &str = "discovering resources...";
    pub const FLOW_ONBOARD_RESOURCES: &str = "Onboard: Select resources";
    pub const FLOW_CORRELATION_PREVIEW: &str = "Correlation Preview";
    pub const FLOW_APPLYING: &str = "Applying changes...";
    pub const FLOW_EMPTY_DISCOVERY: &str = "(no resources found)";
}

pub fn draw(frame: &mut Frame, area: Rect, app: &App, theme: &Theme) {
    if app.data.platform_health.is_empty() {
        let msg = Paragraph::new("No platform health data yet.")
            .style(theme.t7())
            .block(Block::default().borders(Borders::NONE));
        frame.render_widget(msg, area);
        return;
    }

    let chunks = Layout::horizontal([Constraint::Length(28), Constraint::Min(40)])
        .split(area);

    let header_cells = ["Platform", "State"]
        .iter()
        .map(|h| Cell::from(*h).style(theme.t4()));
    let header = Row::new(header_cells);

    let rows: Vec<Row> = app
        .data
        .platform_health
        .iter()
        .enumerate()
        .map(|(i, report)| {
            let is_selected = i == app.selected_row;
            let row_style = if is_selected {
                theme.selected_row()
            } else {
                ratatui::style::Style::default()
            };

            Row::new(vec![
                Cell::from(Span::styled(
                    report.platform.display_name().to_string(),
                    theme.t5(),
                )),
                Cell::from(Span::styled(
                    format!("{} {}", report.state.icon(), report.state.label()),
                    state_style(report.state, theme),
                )),
            ])
            .style(row_style)
        })
        .collect();

    let table = Table::new(
        rows,
        [
            Constraint::Length(10),
            Constraint::Min(14),
        ],
    )
    .header(header)
    .block(Block::default().borders(Borders::NONE))
    .row_highlight_style(theme.selected_row())
    .highlight_symbol("▶ ");

    let mut state = TableState::default();
    state.select(Some(
        app.selected_row
            .min(app.data.platform_health.len().saturating_sub(1)),
    ));
    frame.render_stateful_widget(table, chunks[0], &mut state);

    if let Some(selected) = app.data.platform_health.get(
        app.selected_row
            .min(app.data.platform_health.len().saturating_sub(1)),
    ) {
        let detail = render_detail(selected, app);
        let detail_widget = Paragraph::new(detail)
            .style(theme.t8())
            .wrap(Wrap { trim: false })
            .block(
            Block::default()
                .borders(Borders::LEFT)
                .border_style(theme.panel_border())
                .title(Span::styled(" Auth & Onboard ", theme.t6())),
        );
        frame.render_widget(detail_widget, chunks[1]);
    }
}

fn state_style(state: PlatformHealthState, theme: &Theme) -> ratatui::style::Style {
    match state {
        PlatformHealthState::Ready => theme.success(),
        PlatformHealthState::NoToken => theme.neutral(),
        PlatformHealthState::InvalidToken => theme.failure(),
        PlatformHealthState::ConnectivityError => theme.warning(),
        PlatformHealthState::AccessOrConfigIncomplete => theme.warning(),
    }
}

fn render_detail(report: &PlatformHealthReport, app: &App) -> String {
    let token_source = report
        .token_source
        .clone()
        .unwrap_or_else(|| "none".to_string());

    let mut lines = vec![
        copy::PROVIDER.to_string(),
        format!(
            "  {} ({} {})",
            report.platform.display_name(),
            report.state.icon(),
            report.state.label()
        ),
        copy::TOKEN_SOURCE.to_string(),
        format!("  {token_source}"),
        copy::REASON.to_string(),
        format!("  {}", report.reason),
        copy::NEXT_ACTION.to_string(),
        format!("  {}", report.next_action),
        String::new(),
        copy::ACTIONS.to_string(),
        format!("  {}", copy::ACTION_TOKEN),
        format!("  {}", copy::ACTION_REMOVE),
        format!("  {}", copy::ACTION_SELECT),
    ];
    if app.selected_token_from_env() {
        lines.push(format!("  {}", copy::ENV_WARNING));
    }

    lines.push(String::new());
    lines.push(copy::RESULT.to_string());
    if let Some(msg) = &app.settings_message {
        lines.push(format!("  {msg}"));
    } else {
        lines.push("  (none)".to_string());
    }

    match app.settings_flow {
        SettingsFlowState::TokenEntry => {
            lines.push(String::new());
            lines.push(copy::FLOW_TOKEN_INPUT.to_string());
            lines.push(format!("  {}", "•".repeat(app.token_input.chars().count().min(64))));
            lines.push(format!("  {}", copy::FLOW_TOKEN_HINT));
            lines.push("  Esc to cancel".to_string());
        }
        SettingsFlowState::ValidatingToken => {
            lines.push(String::new());
            lines.push(copy::FLOW_VALIDATING_TOKEN.to_string());
        }
        SettingsFlowState::ProviderActions => {
            lines.push(String::new());
            lines.push(copy::FLOW_ONBOARD_PROVIDERS.to_string());
            for (idx, platform) in pulsos_core::auth::PlatformKind::ALL.iter().enumerate() {
                let pointer = if idx == app.onboarding.platform_cursor {
                    ">"
                } else {
                    " "
                };
                let checked = if app.onboarding.platform_selected.get(idx).copied().unwrap_or(false) {
                    "x"
                } else {
                    " "
                };
                lines.push(format!(" {pointer} [{checked}] {}", platform.display_name()));
            }
            lines.push("  Space toggle  Enter discover  Esc cancel".to_string());
        }
        SettingsFlowState::DiscoveryScanning => {
            lines.push(String::new());
            lines.push(copy::FLOW_DISCOVERING.to_string());
        }
        SettingsFlowState::ResourceSelection => {
            lines.push(String::new());
            lines.push(copy::FLOW_ONBOARD_RESOURCES.to_string());
            render_resource_selection(&mut lines, app);
            lines.push("  Space toggle  Enter preview  Esc cancel".to_string());
        }
        SettingsFlowState::CorrelationReview => {
            lines.push(String::new());
            lines.push(copy::FLOW_CORRELATION_PREVIEW.to_string());
            for line in app.onboarding.correlation_preview.iter().take(16) {
                lines.push(format!("  - {line}"));
            }
            lines.push("  Enter apply  Esc back".to_string());
        }
        SettingsFlowState::Applying => {
            lines.push(String::new());
            lines.push(copy::FLOW_APPLYING.to_string());
        }
        SettingsFlowState::ValidationResult | SettingsFlowState::Idle => {}
    }

    lines.push(String::new());
    lines.push(copy::PROVIDER_STATS.to_string());
    let provider_detail = match &report.details {
        PlatformHealthDetails::GitHub(details) => format!(
            "identity={}  repos {}/{} accessible  orgs {}/{} accessible",
            details.identity.as_deref().unwrap_or("unknown"),
            details.accessible_repos,
            details.configured_repo_checks,
            details.accessible_orgs,
            details.configured_org_checks,
        ),
        PlatformHealthDetails::Railway(details) => format!(
            "identity={}  workspaces {}/{} accessible  projects {}/{} accessible",
            details.identity.as_deref().unwrap_or("unknown"),
            details.accessible_workspaces,
            details.configured_workspace_checks,
            details.accessible_projects,
            details.configured_project_checks,
        ),
        PlatformHealthDetails::Vercel(details) => format!(
            "identity={}  teams {}/{} accessible  projects {}/{} accessible",
            details.identity.as_deref().unwrap_or("unknown"),
            details.accessible_teams,
            details.configured_team_checks,
            details.accessible_projects,
            details.configured_project_checks,
        ),
        PlatformHealthDetails::None => "no provider-specific details".to_string(),
    };
    lines.push(format!("  {provider_detail}"));

    lines.join("\n")
}

fn render_resource_selection(lines: &mut Vec<String>, app: &App) {
    let mut flat_index = 0usize;
    for item in &app.onboarding.github {
        push_resource_line(lines, app, flat_index, "GH", &item.resource.display_name, item.selected);
        flat_index += 1;
    }
    for item in &app.onboarding.railway {
        push_resource_line(lines, app, flat_index, "RW", &item.resource.display_name, item.selected);
        flat_index += 1;
    }
    for item in &app.onboarding.vercel {
        let display = if let Some(link) = &item.linked_repo {
            format!("{} -> {}", item.resource.display_name, link)
        } else {
            item.resource.display_name.clone()
        };
        push_resource_line(lines, app, flat_index, "VC", &display, item.selected);
        flat_index += 1;
    }
    if flat_index == 0 {
        lines.push(format!("  {}", copy::FLOW_EMPTY_DISCOVERY));
    }
}

fn push_resource_line(
    lines: &mut Vec<String>,
    app: &App,
    flat_index: usize,
    prefix: &str,
    display: &str,
    selected: bool,
) {
    let pointer = if flat_index == app.onboarding.resource_cursor {
        ">"
    } else {
        " "
    };
    let checked = if selected { "x" } else { " " };
    lines.push(format!(" {pointer} [{checked}] {prefix} {}", truncate(display, 80)));
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let mut out: String = s.chars().take(max.saturating_sub(1)).collect();
        out.push('…');
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::app::{App, DataSnapshot};
    use crate::tui::log_buffer::LogRingBuffer;
    use chrono::Utc;
    use pulsos_core::auth::PlatformKind;
    use pulsos_core::config::types::TuiConfig;
    use pulsos_core::health::PlatformHealthDetails;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    fn health_report(token_source: Option<&str>) -> PlatformHealthReport {
        PlatformHealthReport {
            platform: PlatformKind::GitHub,
            state: PlatformHealthState::Ready,
            reason: "ready".to_string(),
            next_action: "none".to_string(),
            token_source: token_source.map(str::to_string),
            last_checked_at: Utc::now(),
            details: PlatformHealthDetails::None,
        }
    }

    #[test]
    fn settings_tab_renders_health_rows() {
        let backend = TestBackend::new(120, 20);
        let mut terminal = Terminal::new(backend).unwrap();

        let mut data = DataSnapshot::default();
        data.platform_health = vec![health_report(Some("keyring"))];

        let app = App::new(data, TuiConfig::default(), LogRingBuffer::new());
        let theme = Theme::dark();

        terminal
            .draw(|frame| draw(frame, frame.area(), &app, &theme))
            .unwrap();

        let buf = terminal.backend().buffer();
        let mut text = String::new();
        for y in buf.area.top()..buf.area.bottom() {
            for x in buf.area.left()..buf.area.right() {
                text.push_str(buf.cell((x, y)).map(|c| c.symbol()).unwrap_or(" "));
            }
        }

        assert!(text.contains("GitHub"));
        assert!(text.contains("Ready"));
    }

    #[test]
    fn detail_copy_uses_normalized_sections_and_actions() {
        let report = health_report(Some("keyring"));
        let mut data = DataSnapshot::default();
        data.platform_health = vec![report.clone()];
        let app = App::new(data, TuiConfig::default(), LogRingBuffer::new());

        let detail = render_detail(&report, &app);
        for heading in [
            copy::PROVIDER,
            copy::TOKEN_SOURCE,
            copy::REASON,
            copy::NEXT_ACTION,
            copy::ACTIONS,
            copy::RESULT,
            copy::PROVIDER_STATS,
        ] {
            assert!(detail.contains(heading), "missing heading {heading}");
        }
        assert!(detail.contains(copy::ACTION_TOKEN));
        assert!(detail.contains(copy::ACTION_REMOVE));
        assert!(detail.contains(copy::ACTION_SELECT));
    }

    #[test]
    fn detail_shows_env_override_warning_when_env_token_is_active() {
        let report = health_report(Some("GITHUB_TOKEN"));
        let mut data = DataSnapshot::default();
        data.platform_health = vec![report.clone()];
        let app = App::new(data, TuiConfig::default(), LogRingBuffer::new());

        let detail = render_detail(&report, &app);
        assert!(detail.contains(copy::ENV_WARNING));
    }
}
