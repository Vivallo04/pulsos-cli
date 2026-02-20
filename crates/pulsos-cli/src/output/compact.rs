use pulsos_core::domain::deployment::DeploymentStatus;
use pulsos_core::domain::project::CorrelatedEvent;
use std::io::IsTerminal;

/// Render correlated events in compact single-line format (§4.28):
///
/// ```text
/// my-saas       ✓ ✓ ✓
/// api-core      ✓ ✓ —
/// auth-service  ✗ ✓ —
/// ```
pub fn render_correlated(events: &[CorrelatedEvent]) {
    if events.is_empty() {
        println!("No deployment events found.");
        return;
    }

    let colored = std::io::stdout().is_terminal() && std::env::var("NO_COLOR").is_err();

    for c in events {
        // Project name: prefer config project_name, then platform titles, then SHA
        let name_raw = c
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

        let gh_sym = c
            .github
            .as_ref()
            .map(|e| status_symbol_colored(&e.status, colored))
            .unwrap_or_else(|| dim_str("—", colored));

        let rw_sym = c
            .railway
            .as_ref()
            .map(|e| status_symbol_colored(&e.status, colored))
            .unwrap_or_else(|| dim_str("—", colored));

        let vc_sym = c
            .vercel
            .as_ref()
            .map(|e| status_symbol_colored(&e.status, colored))
            .unwrap_or_else(|| dim_str("—", colored));

        // Use a fixed-width name column, truncated to 16 chars
        let name_display = truncate(name_raw, 16);
        println!("{:<16}  {} {} {}", name_display, gh_sym, rw_sym, vc_sym,);
    }
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

/// Return a colored status symbol string.
fn status_symbol_colored(status: &DeploymentStatus, colored: bool) -> String {
    let (sym, code) = match status {
        DeploymentStatus::Success => ("✓", "\x1b[38;2;52;211;153m"),
        DeploymentStatus::Failed => ("✗", "\x1b[38;2;248;113;113m"),
        DeploymentStatus::InProgress => ("◌", "\x1b[38;2;96;165;250m"),
        DeploymentStatus::Queued => ("⏸", "\x1b[38;2;156;163;175m"),
        DeploymentStatus::Cancelled => ("—", "\x1b[38;2;156;163;175m"),
        DeploymentStatus::Skipped => ("—", "\x1b[38;2;156;163;175m"),
        DeploymentStatus::ActionRequired => ("⚠", "\x1b[38;2;251;191;36m"),
        DeploymentStatus::Sleeping => ("●", "\x1b[38;2;156;163;175m"),
        DeploymentStatus::Unknown(_) => ("?", "\x1b[38;2;156;163;175m"),
    };
    if colored {
        format!("{code}{sym}\x1b[0m")
    } else {
        sym.to_string()
    }
}

fn dim_str(s: &str, colored: bool) -> String {
    if colored {
        format!("\x1b[38;2;85;85;85m{s}\x1b[0m")
    } else {
        s.to_string()
    }
}
