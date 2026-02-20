use chrono::Utc;
use pulsos_core::domain::deployment::DeploymentStatus;
use pulsos_core::domain::project::CorrelatedEvent;
use std::io::IsTerminal;

// ── Status badge ─────────────────────────────────────────────────────────────

/// Returns `"{symbol}{label}"` with ANSI color codes when `colored` is true.
///
/// Colors follow the design system §1.2 RGB values.
fn status_badge_ansi(status: &DeploymentStatus, colored: bool) -> String {
    let (symbol, label, code) = match status {
        DeploymentStatus::Success => ("✓ ", "passed", "\x1b[38;2;52;211;153m"),
        DeploymentStatus::Failed => ("✗ ", "failed", "\x1b[38;2;248;113;113m"),
        DeploymentStatus::InProgress => ("◌ ", "building", "\x1b[38;2;96;165;250m"),
        DeploymentStatus::Queued => ("⏸ ", "queued", "\x1b[38;2;156;163;175m"),
        DeploymentStatus::Cancelled => ("— ", "cancelled", "\x1b[38;2;156;163;175m"),
        DeploymentStatus::Skipped => ("— ", "skipped", "\x1b[38;2;156;163;175m"),
        DeploymentStatus::ActionRequired => ("⚠ ", "action", "\x1b[38;2;251;191;36m"),
        DeploymentStatus::Sleeping => ("● ", "sleeping", "\x1b[38;2;156;163;175m"),
        DeploymentStatus::Unknown(s) => {
            let label = s.as_str();
            if colored {
                return format!("\x1b[38;2;156;163;175m? {label}\x1b[0m");
            } else {
                return format!("? {label}");
            }
        }
    };
    if colored {
        format!("{code}{symbol}{label}\x1b[0m")
    } else {
        format!("{symbol}{label}")
    }
}

/// Plain-text status label (no color, no symbol) — for non-terminal / pipe output.
#[allow(dead_code)]
pub(crate) fn status_indicator(status: &DeploymentStatus) -> String {
    match status {
        DeploymentStatus::Success => "passed".into(),
        DeploymentStatus::Failed => "failed".into(),
        DeploymentStatus::InProgress => "building".into(),
        DeploymentStatus::Queued => "queued".into(),
        DeploymentStatus::Cancelled => "cancelled".into(),
        DeploymentStatus::Skipped => "skipped".into(),
        DeploymentStatus::ActionRequired => "action".into(),
        DeploymentStatus::Sleeping => "sleeping".into(),
        DeploymentStatus::Unknown(s) => s.clone(),
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

pub(crate) fn format_age(created_at: chrono::DateTime<Utc>) -> String {
    let diff = Utc::now() - created_at;
    let secs = diff.num_seconds();
    if secs < 60 {
        "just now".into()
    } else if secs < 3600 {
        format!("{}m ago", secs / 60)
    } else if secs < 86400 {
        format!("{}h ago", secs / 3600)
    } else {
        format!("{}d ago", secs / 86400)
    }
}

pub(crate) fn format_duration(secs: u64) -> String {
    if secs < 60 {
        format!("{secs}s")
    } else if secs < 3600 {
        format!("{}m {}s", secs / 60, secs % 60)
    } else {
        format!("{}h {}m", secs / 3600, (secs % 3600) / 60)
    }
}

// ── Color helpers for health score ───────────────────────────────────────────

fn health_color_ansi(score: u8, colored: bool) -> (&'static str, &'static str) {
    if !colored {
        return ("", "");
    }
    if score >= 90 {
        ("\x1b[38;2;52;211;153m", "\x1b[0m") // status.success
    } else if score >= 70 {
        ("\x1b[38;2;251;191;36m", "\x1b[0m") // status.warning
    } else {
        ("\x1b[38;2;248;113;113m", "\x1b[0m") // status.failure
    }
}

fn dim_ansi(colored: bool) -> (&'static str, &'static str) {
    if colored {
        ("\x1b[38;2;85;85;85m", "\x1b[0m")
    } else {
        ("", "")
    }
}

fn bold_subtle_ansi(colored: bool) -> (&'static str, &'static str) {
    if colored {
        ("\x1b[1;38;2;119;119;119m", "\x1b[0m")
    } else {
        ("", "")
    }
}

// ── Table renderer ────────────────────────────────────────────────────────────

/// Column widths (§3.3)
const W_PROJECT: usize = 16;
const W_PLATFORM: usize = 12;
const W_HEALTH: usize = 6;
#[allow(dead_code)]
const W_BRANCH: usize = 12;
const W_AGE: usize = 8;
const GAP: usize = 2;

/// Render the correlated events as a formatted status table.
///
/// Uses ANSI status badges when stdout is a terminal and NO_COLOR is not set.
pub fn render_correlated(events: &[CorrelatedEvent]) {
    if events.is_empty() {
        println!("No deployment events found.");
        return;
    }

    let colored = std::io::stdout().is_terminal() && std::env::var("NO_COLOR").is_err();

    let (dim, dim_reset) = dim_ansi(colored);
    let (hdr, hdr_reset) = bold_subtle_ansi(colored);

    // Header line
    println!(
        "{hdr}{:<W_PROJECT$}{:GAP$}{:<W_PLATFORM$}{:GAP$}{:<W_PLATFORM$}{:GAP$}{:<W_PLATFORM$}{:GAP$}{:>W_HEALTH$}{hdr_reset}",
        "Project", "", "GitHub CI", "", "Railway", "", "Vercel", "", "Health",
        W_PROJECT = W_PROJECT,
        GAP = GAP,
        W_PLATFORM = W_PLATFORM,
        W_HEALTH = W_HEALTH,
    );

    // Underline (─ repeated for each column + gaps)
    let total = W_PROJECT + GAP + W_PLATFORM + GAP + W_PLATFORM + GAP + W_PLATFORM + GAP + W_HEALTH;
    println!("{dim}{}{dim_reset}", "─".repeat(total));

    for c in events {
        // Project name: prefer config project_name, then platform titles, then SHA
        let project_raw = c
            .project_name
            .as_deref()
            .or_else(|| c.vercel.as_ref().and_then(|e| e.title.as_deref()))
            .or_else(|| c.railway.as_ref().and_then(|e| e.title.as_deref()))
            .or_else(|| c.github.as_ref().and_then(|e| e.title.as_deref()))
            .or_else(|| {
                c.commit_sha
                    .as_deref()
                    .map(|s| if s.len() > 8 { &s[..8] } else { s })
            })
            .unwrap_or("-");
        let project = truncate(project_raw, W_PROJECT);

        let gh = c
            .github
            .as_ref()
            .map(|e| status_badge_ansi(&e.status, colored))
            .unwrap_or_else(|| format!("{dim}—{dim_reset}"));

        let rw = c
            .railway
            .as_ref()
            .map(|e| status_badge_ansi(&e.status, colored))
            .unwrap_or_else(|| format!("{dim}—{dim_reset}"));

        let vc = c
            .vercel
            .as_ref()
            .map(|e| status_badge_ansi(&e.status, colored))
            .unwrap_or_else(|| format!("{dim}—{dim_reset}"));

        // Compute age
        let age_raw = if c.is_stale {
            format!("{} ●", format_age(c.timestamp))
        } else {
            format_age(c.timestamp)
        };
        let age = truncate(&age_raw, W_AGE);

        // Left-pad to column width (strip ANSI for width calc)
        let gh_padded = pad_right_ansi(&gh, W_PLATFORM);
        let rw_padded = pad_right_ansi(&rw, W_PLATFORM);
        let vc_padded = pad_right_ansi(&vc, W_PLATFORM);

        println!(
            "{:<W_PROJECT$}{:GAP$}{}{:GAP$}{}{:GAP$}{}{:GAP$}{}",
            project,
            "",
            gh_padded,
            "",
            rw_padded,
            "",
            vc_padded,
            "",
            age,
            W_PROJECT = W_PROJECT,
            GAP = GAP,
        );
    }
}

/// Render per-project health score summary.
pub fn render_health_scores(scores: &[(String, u8)]) {
    if scores.is_empty() {
        return;
    }

    let colored = std::io::stdout().is_terminal() && std::env::var("NO_COLOR").is_err();

    let (hdr, hdr_reset) = bold_subtle_ansi(colored);
    let (dim, dim_reset) = dim_ansi(colored);

    println!();
    println!("{hdr}Project Health{hdr_reset}");
    println!("{dim}{}{dim_reset}", "─".repeat(45));

    let name_width = scores
        .iter()
        .map(|(n, _)| n.len())
        .max()
        .unwrap_or(0)
        .max(7);

    for (name, score) in scores {
        let (score_open, score_close) = health_color_ansi(*score, colored);
        let filled = (*score as usize * 20) / 100;
        let empty = 20 - filled;
        let bar = format!("{}{}", "█".repeat(filled), "░".repeat(empty));
        println!(
            "  {:<width$}  {}{:>3}{}  {}",
            name,
            score_open,
            score,
            score_close,
            bar,
            width = name_width
        );
    }
}

// ── Internal helpers ──────────────────────────────────────────────────────────

/// Truncate a string to `max` chars (works on char boundaries).
fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let mut out: String = s.chars().take(max.saturating_sub(1)).collect();
        out.push('…');
        out
    }
}

/// Pad a string (which may contain ANSI codes) to `width` visible chars.
/// Counts visible chars by stripping ANSI escape sequences for measurement.
fn pad_right_ansi(s: &str, width: usize) -> String {
    let visible_len = strip_ansi_len(s);
    if visible_len >= width {
        s.to_string()
    } else {
        format!("{}{}", s, " ".repeat(width - visible_len))
    }
}

/// Count visible characters in a string that may contain ANSI codes.
fn strip_ansi_len(s: &str) -> usize {
    let mut len = 0usize;
    let mut in_escape = false;
    for ch in s.chars() {
        if ch == '\x1b' {
            in_escape = true;
        } else if in_escape {
            if ch == 'm' {
                in_escape = false;
            }
        } else {
            len += ch.len_utf8().min(1); // approximate: count code points
        }
    }
    len
}
