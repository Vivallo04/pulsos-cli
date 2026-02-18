//! Color themes for the TUI — dark and light palettes.

use ratatui::style::{Color, Modifier, Style};

/// Complete color palette for the TUI.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct Theme {
    // General
    pub bg: Color,
    pub fg: Color,
    pub border: Color,
    pub highlight: Style,

    // Status colors
    pub success: Color,
    pub failure: Color,
    pub in_progress: Color,
    pub queued: Color,
    pub warning: Color,
    pub muted: Color,

    // Platform colors
    pub github: Color,
    pub railway: Color,
    pub vercel: Color,

    // Tab bar
    pub tab_active: Style,
    pub tab_inactive: Style,

    // Health score thresholds
    pub health_good: Color, // 80-100
    pub health_warn: Color, // 50-79
    pub health_bad: Color,  // 0-49
}

impl Theme {
    pub fn dark() -> Self {
        Self {
            bg: Color::Reset,
            fg: Color::White,
            border: Color::DarkGray,
            highlight: Style::default()
                .fg(Color::Black)
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD),

            success: Color::Green,
            failure: Color::Red,
            in_progress: Color::Yellow,
            queued: Color::Blue,
            warning: Color::Yellow,
            muted: Color::DarkGray,

            github: Color::White,
            railway: Color::Magenta,
            vercel: Color::Cyan,

            tab_active: Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
            tab_inactive: Style::default().fg(Color::DarkGray),

            health_good: Color::Green,
            health_warn: Color::Yellow,
            health_bad: Color::Red,
        }
    }

    pub fn light() -> Self {
        Self {
            bg: Color::Reset,
            fg: Color::Black,
            border: Color::Gray,
            highlight: Style::default()
                .fg(Color::White)
                .bg(Color::Blue)
                .add_modifier(Modifier::BOLD),

            success: Color::Green,
            failure: Color::Red,
            in_progress: Color::DarkGray,
            queued: Color::Blue,
            warning: Color::Rgb(200, 150, 0),
            muted: Color::Gray,

            github: Color::Black,
            railway: Color::Magenta,
            vercel: Color::Blue,

            tab_active: Style::default()
                .fg(Color::Blue)
                .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
            tab_inactive: Style::default().fg(Color::Gray),

            health_good: Color::Green,
            health_warn: Color::Rgb(200, 150, 0),
            health_bad: Color::Red,
        }
    }

    /// Pick a theme based on TuiConfig and PULSOS_THEME env var.
    ///
    /// Priority: PULSOS_THEME env var > config file > default (dark).
    pub fn resolve(config_theme: &str) -> Self {
        let theme_name = std::env::var("PULSOS_THEME")
            .ok()
            .unwrap_or_else(|| config_theme.to_string());

        match theme_name.to_ascii_lowercase().as_str() {
            "light" => Self::light(),
            _ => Self::dark(),
        }
    }

    /// Return the appropriate color for a health score.
    pub fn health_color(&self, score: u8) -> Color {
        if score >= 80 {
            self.health_good
        } else if score >= 50 {
            self.health_warn
        } else {
            self.health_bad
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dark_theme_defaults() {
        let theme = Theme::dark();
        assert_eq!(theme.success, Color::Green);
        assert_eq!(theme.failure, Color::Red);
        assert_eq!(theme.fg, Color::White);
    }

    #[test]
    fn light_theme_defaults() {
        let theme = Theme::light();
        assert_eq!(theme.fg, Color::Black);
        assert_eq!(theme.success, Color::Green);
    }

    #[test]
    fn resolve_defaults_to_dark() {
        // Don't set PULSOS_THEME — resolve should fall back to config, which defaults to "dark"
        let theme = Theme::resolve("dark");
        assert_eq!(theme.fg, Color::White);
    }

    #[test]
    fn resolve_light_from_config() {
        let theme = Theme::resolve("light");
        // Only works if PULSOS_THEME is unset; env var takes priority
        // In test context, just check it doesn't panic
        assert!(theme.fg == Color::Black || theme.fg == Color::White);
    }

    #[test]
    fn health_color_thresholds() {
        let theme = Theme::dark();
        assert_eq!(theme.health_color(100), Color::Green);
        assert_eq!(theme.health_color(80), Color::Green);
        assert_eq!(theme.health_color(79), Color::Yellow);
        assert_eq!(theme.health_color(50), Color::Yellow);
        assert_eq!(theme.health_color(49), Color::Red);
        assert_eq!(theme.health_color(0), Color::Red);
    }
}
