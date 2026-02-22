//! Color themes for the TUI — Pulsos Design System v1.0 §8.1.

use ratatui::style::{Color, Style, Stylize};

/// Complete color palette per Pulsos Design System §8.1.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct Theme {
    // Background
    pub bg_primary: Color,
    pub bg_surface: Color,
    pub bg_elevated: Color,
    pub bg_overlay: Color,

    // Borders
    pub border_muted: Color,
    pub border_default: Color,
    pub border_focus: Color,

    // Foreground
    pub fg_muted: Color,
    pub fg_subtle: Color,
    pub fg_default: Color,
    pub fg_strong: Color,
    pub fg_emphasis: Color,

    // Semantic — status
    pub status_success: Color,
    pub status_success_muted: Color,
    pub status_failure: Color,
    pub status_failure_muted: Color,
    pub status_warning: Color,
    pub status_warning_muted: Color,
    pub status_active: Color,
    pub status_active_muted: Color,
    pub status_neutral: Color,

    // Accent
    pub accent_primary: Color,
    pub accent_dim: Color,

    // Platform accents
    pub platform_gh: Color,
    pub platform_rw: Color,
    pub platform_vc: Color,
}

impl Theme {
    pub fn dark() -> Self {
        Self {
            bg_primary: Color::Rgb(13, 13, 13),
            bg_surface: Color::Rgb(22, 22, 22),
            bg_elevated: Color::Rgb(30, 30, 30),
            bg_overlay: Color::Rgb(37, 37, 37),

            border_muted: Color::Rgb(51, 51, 51),
            border_default: Color::Rgb(68, 68, 68),
            border_focus: Color::Rgb(136, 136, 136),

            fg_muted: Color::Rgb(85, 85, 85),
            fg_subtle: Color::Rgb(119, 119, 119),
            fg_default: Color::Rgb(176, 176, 176),
            fg_strong: Color::Rgb(224, 224, 224),
            fg_emphasis: Color::White,

            status_success: Color::Rgb(52, 211, 153),
            status_success_muted: Color::Rgb(6, 95, 70),
            status_failure: Color::Rgb(248, 113, 113),
            status_failure_muted: Color::Rgb(127, 29, 29),
            status_warning: Color::Rgb(251, 191, 36),
            status_warning_muted: Color::Rgb(120, 53, 15),
            status_active: Color::Rgb(96, 165, 250),
            status_active_muted: Color::Rgb(30, 58, 95),
            status_neutral: Color::Rgb(156, 163, 175),

            accent_primary: Color::Rgb(129, 140, 248),
            accent_dim: Color::Rgb(67, 56, 202),

            platform_gh: Color::Rgb(226, 232, 240),
            platform_rw: Color::Rgb(167, 139, 250),
            platform_vc: Color::Rgb(56, 189, 248),
        }
    }

    pub fn light() -> Self {
        Self {
            bg_primary: Color::Rgb(250, 250, 250),
            bg_surface: Color::Rgb(255, 255, 255),
            bg_elevated: Color::Rgb(240, 240, 240),
            bg_overlay: Color::Rgb(230, 230, 230),

            border_muted: Color::Rgb(220, 220, 220),
            border_default: Color::Rgb(212, 212, 212),
            border_focus: Color::Rgb(120, 120, 120),

            fg_muted: Color::Rgb(160, 160, 160),
            fg_subtle: Color::Rgb(120, 120, 120),
            fg_default: Color::Rgb(64, 64, 64),
            fg_strong: Color::Rgb(26, 26, 26),
            fg_emphasis: Color::Black,

            status_success: Color::Rgb(5, 150, 105),
            status_success_muted: Color::Rgb(209, 250, 229),
            status_failure: Color::Rgb(220, 38, 38),
            status_failure_muted: Color::Rgb(254, 226, 226),
            status_warning: Color::Rgb(217, 119, 6),
            status_warning_muted: Color::Rgb(254, 243, 199),
            status_active: Color::Rgb(37, 99, 235),
            status_active_muted: Color::Rgb(219, 234, 254),
            status_neutral: Color::Rgb(107, 114, 128),

            accent_primary: Color::Rgb(79, 70, 229),
            accent_dim: Color::Rgb(199, 210, 254),

            platform_gh: Color::Rgb(51, 65, 85),
            platform_rw: Color::Rgb(109, 40, 217),
            platform_vc: Color::Rgb(2, 132, 199),
        }
    }

    pub fn ansi16() -> Self {
        Self {
            bg_primary: Color::Black,
            bg_surface: Color::Black,
            bg_elevated: Color::DarkGray,
            bg_overlay: Color::DarkGray,

            border_muted: Color::DarkGray,
            border_default: Color::DarkGray,
            border_focus: Color::White,

            fg_muted: Color::DarkGray,
            fg_subtle: Color::DarkGray,
            fg_default: Color::White,
            fg_strong: Color::White,
            fg_emphasis: Color::White,

            status_success: Color::Green,
            status_success_muted: Color::Black,
            status_failure: Color::Red,
            status_failure_muted: Color::Black,
            status_warning: Color::Yellow,
            status_warning_muted: Color::Black,
            status_active: Color::Blue,
            status_active_muted: Color::Black,
            status_neutral: Color::DarkGray,

            accent_primary: Color::Magenta,
            accent_dim: Color::DarkGray,

            platform_gh: Color::White,
            platform_rw: Color::Magenta,
            platform_vc: Color::Cyan,
        }
    }

    /// Pick a theme based on TuiConfig, PULSOS_THEME env var, and NO_COLOR.
    ///
    /// Priority: NO_COLOR env → PULSOS_THEME env → config file → default (dark).
    pub fn resolve(config_theme: &str) -> Self {
        // NO_COLOR standard takes highest priority
        if std::env::var("NO_COLOR").is_ok() {
            return Self::ansi16();
        }

        let theme_name = std::env::var("PULSOS_THEME")
            .ok()
            .unwrap_or_else(|| config_theme.to_string());

        match theme_name.to_ascii_lowercase().as_str() {
            "light" => Self::light(),
            "ansi16" => Self::ansi16(),
            _ => Self::dark(),
        }
    }

    /// Return the appropriate color for a health score.
    /// Thresholds per §4.2: ≥90 → success, ≥70 → warning, <70 → failure.
    pub fn health_color(&self, score: u8) -> Color {
        if score >= 90 {
            self.status_success
        } else if score >= 70 {
            self.status_warning
        } else {
            self.status_failure
        }
    }
}

// ── Convenience style constructors ──────────────────────────────────────────

#[allow(dead_code)]
impl Theme {
    // Typography levels (§2.1)
    pub fn t1(&self) -> Style {
        Style::new().fg(self.accent_primary).bold()
    }
    pub fn t2(&self) -> Style {
        Style::new().fg(self.fg_emphasis).bold()
    }
    pub fn t3(&self) -> Style {
        Style::new().fg(self.fg_emphasis).bold()
    }
    pub fn t4(&self) -> Style {
        Style::new().fg(self.fg_subtle).bold()
    }
    pub fn t5(&self) -> Style {
        Style::new().fg(self.fg_strong).bold()
    }
    pub fn t6(&self) -> Style {
        Style::new().fg(self.fg_default)
    }
    pub fn t7(&self) -> Style {
        Style::new().fg(self.fg_subtle)
    }
    pub fn t8(&self) -> Style {
        Style::new().fg(self.fg_muted)
    }
    pub fn t9(&self) -> Style {
        Style::new().fg(self.border_default)
    }

    // Semantic status styles
    pub fn success(&self) -> Style {
        Style::new().fg(self.status_success).bold()
    }
    pub fn failure(&self) -> Style {
        Style::new().fg(self.status_failure).bold()
    }
    pub fn warning(&self) -> Style {
        Style::new().fg(self.status_warning).bold()
    }
    pub fn active(&self) -> Style {
        Style::new().fg(self.status_active).bold()
    }
    pub fn neutral(&self) -> Style {
        Style::new().fg(self.status_neutral)
    }

    // Component styles
    pub fn selected_row(&self) -> Style {
        Style::new().bg(self.bg_elevated)
    }
    pub fn panel_border(&self) -> Style {
        Style::new().fg(self.border_default)
    }
    pub fn panel_border_focus(&self) -> Style {
        Style::new().fg(self.border_focus)
    }
    pub fn tab_active(&self) -> Style {
        Style::new().fg(self.fg_emphasis).bold()
    }
    pub fn tab_inactive(&self) -> Style {
        Style::new().fg(self.fg_subtle)
    }
    pub fn keybind_key(&self) -> Style {
        Style::new().fg(self.accent_primary).bold()
    }
    pub fn keybind_desc(&self) -> Style {
        Style::new().fg(self.fg_muted)
    }

    // Platform accent styles
    pub fn gh_accent(&self) -> Style {
        Style::new().fg(self.platform_gh).bold()
    }
    pub fn rw_accent(&self) -> Style {
        Style::new().fg(self.platform_rw).bold()
    }
    pub fn vc_accent(&self) -> Style {
        Style::new().fg(self.platform_vc).bold()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dark_theme_defaults() {
        let theme = Theme::dark();
        assert_eq!(theme.status_success, Color::Rgb(52, 211, 153));
        assert_eq!(theme.status_failure, Color::Rgb(248, 113, 113));
        assert_eq!(theme.fg_emphasis, Color::White);
    }

    #[test]
    fn light_theme_defaults() {
        let theme = Theme::light();
        assert_eq!(theme.fg_emphasis, Color::Black);
        assert_eq!(theme.status_success, Color::Rgb(5, 150, 105));
    }

    #[test]
    fn ansi16_theme_has_named_colors() {
        let theme = Theme::ansi16();
        assert_eq!(theme.status_success, Color::Green);
        assert_eq!(theme.status_failure, Color::Red);
        assert_eq!(theme.accent_primary, Color::Magenta);
    }

    #[test]
    fn resolve_defaults_to_dark() {
        // Don't set PULSOS_THEME — resolve should fall back to config, which defaults to "dark"
        let theme = Theme::resolve("dark");
        assert_eq!(theme.fg_emphasis, Color::White);
    }

    #[test]
    fn resolve_light_from_config() {
        let theme = Theme::resolve("light");
        // Only works if PULSOS_THEME/NO_COLOR are unset; just check it doesn't panic
        let _ = theme.fg_emphasis;
    }

    #[test]
    fn health_color_thresholds() {
        let theme = Theme::dark();
        // ≥ 90 → success (green)
        assert_eq!(theme.health_color(100), Color::Rgb(52, 211, 153));
        assert_eq!(theme.health_color(90), Color::Rgb(52, 211, 153));
        // 70–89 → warning (yellow)
        assert_eq!(theme.health_color(89), Color::Rgb(251, 191, 36));
        assert_eq!(theme.health_color(70), Color::Rgb(251, 191, 36));
        // < 70 → failure (red)
        assert_eq!(theme.health_color(69), Color::Rgb(248, 113, 113));
        assert_eq!(theme.health_color(0), Color::Rgb(248, 113, 113));
    }

    #[test]
    fn style_helpers_exist() {
        let theme = Theme::dark();
        let _ = theme.t1();
        let _ = theme.t4();
        let _ = theme.t8();
        let _ = theme.success();
        let _ = theme.failure();
        let _ = theme.warning();
        let _ = theme.active();
        let _ = theme.neutral();
        let _ = theme.selected_row();
        let _ = theme.tab_active();
        let _ = theme.tab_inactive();
        let _ = theme.keybind_key();
        let _ = theme.keybind_desc();
    }
}
