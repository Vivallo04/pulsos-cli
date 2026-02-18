use chrono::Utc;
use pulsos_core::domain::deployment::DeploymentStatus;
use pulsos_core::domain::project::{Confidence, CorrelatedEvent};

pub fn render_correlated(events: &[CorrelatedEvent]) {
    if events.is_empty() {
        println!("No deployment events found.");
        return;
    }

    for c in events {
        let conf = match c.confidence {
            Confidence::Exact => "EXACT",
            Confidence::High => " HIGH",
            Confidence::Low => "  LOW",
            Confidence::Unmatched => "    ?",
        };

        let sha = c
            .commit_sha
            .as_deref()
            .map(|s| if s.len() > 7 { &s[..7] } else { s })
            .unwrap_or("-------");

        let gh = c
            .github
            .as_ref()
            .map(|e| status_compact(&e.status))
            .unwrap_or("--");

        let rw = c
            .railway
            .as_ref()
            .map(|e| status_compact(&e.status))
            .unwrap_or("--");

        let vc = c
            .vercel
            .as_ref()
            .map(|e| status_compact(&e.status))
            .unwrap_or("--");

        let branch = c
            .github
            .as_ref()
            .and_then(|e| e.branch.as_deref())
            .or_else(|| c.vercel.as_ref().and_then(|e| e.branch.as_deref()))
            .unwrap_or("-");

        let age = format_age_compact(c.timestamp);

        println!("[{conf}] {sha}  GH:{gh}  RW:{rw}  VC:{vc}  {branch}  {age}");
    }
}

fn status_compact(status: &DeploymentStatus) -> &'static str {
    match status {
        DeploymentStatus::Success => "OK",
        DeploymentStatus::Failed => "FAIL",
        DeploymentStatus::InProgress => "RUN",
        DeploymentStatus::Queued => "QUE",
        DeploymentStatus::Cancelled => "CAN",
        DeploymentStatus::Skipped => "SKP",
        DeploymentStatus::ActionRequired => "ACT",
        DeploymentStatus::Sleeping => "SLP",
        DeploymentStatus::Unknown(_) => "???",
    }
}

fn format_age_compact(created_at: chrono::DateTime<Utc>) -> String {
    let diff = Utc::now() - created_at;
    let secs = diff.num_seconds();
    if secs < 60 {
        "now".into()
    } else if secs < 3600 {
        format!("{}m", secs / 60)
    } else if secs < 86400 {
        format!("{}h", secs / 3600)
    } else {
        format!("{}d", secs / 86400)
    }
}
