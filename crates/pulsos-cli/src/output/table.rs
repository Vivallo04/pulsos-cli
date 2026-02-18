use chrono::Utc;
use pulsos_core::domain::deployment::DeploymentStatus;
use pulsos_core::domain::project::CorrelatedEvent;
use tabled::settings::Style;
use tabled::{Table, Tabled};

#[derive(Tabled)]
struct CorrelatedRow {
    #[tabled(rename = "Conf")]
    confidence: String,
    #[tabled(rename = "SHA")]
    sha: String,
    #[tabled(rename = "GitHub")]
    github: String,
    #[tabled(rename = "Railway")]
    railway: String,
    #[tabled(rename = "Vercel")]
    vercel: String,
    #[tabled(rename = "Branch")]
    branch: String,
    #[tabled(rename = "Age")]
    age: String,
}

pub fn render_correlated(events: &[CorrelatedEvent]) {
    if events.is_empty() {
        println!("No deployment events found.");
        return;
    }

    let rows: Vec<CorrelatedRow> = events
        .iter()
        .map(|c| {
            let sha = c
                .commit_sha
                .as_deref()
                .map(|s| if s.len() > 7 { &s[..7] } else { s })
                .unwrap_or("-")
                .to_string();

            let github = c
                .github
                .as_ref()
                .map(|e| status_indicator(&e.status))
                .unwrap_or_else(|| "-".into());

            let railway = c
                .railway
                .as_ref()
                .map(|e| status_indicator(&e.status))
                .unwrap_or_else(|| "-".into());

            let vercel = c
                .vercel
                .as_ref()
                .map(|e| status_indicator(&e.status))
                .unwrap_or_else(|| "-".into());

            let branch = c
                .github
                .as_ref()
                .and_then(|e| e.branch.clone())
                .or_else(|| c.vercel.as_ref().and_then(|e| e.branch.clone()))
                .unwrap_or_else(|| "-".into());

            CorrelatedRow {
                confidence: c.confidence.to_string(),
                sha,
                github,
                railway,
                vercel,
                branch,
                age: format_age(c.timestamp),
            }
        })
        .collect();

    let table = Table::new(&rows).with(Style::rounded()).to_string();
    println!("{table}");
}

pub(crate) fn status_indicator(status: &DeploymentStatus) -> String {
    match status {
        DeploymentStatus::Success => "OK".into(),
        DeploymentStatus::Failed => "FAIL".into(),
        DeploymentStatus::InProgress => "RUNNING".into(),
        DeploymentStatus::Queued => "QUEUED".into(),
        DeploymentStatus::Cancelled => "CANCEL".into(),
        DeploymentStatus::Skipped => "SKIP".into(),
        DeploymentStatus::ActionRequired => "ACTION".into(),
        DeploymentStatus::Sleeping => "SLEEP".into(),
        DeploymentStatus::Unknown(s) => s.clone(),
    }
}

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

/// Render a per-project health score summary.
///
/// Example output:
/// ```
/// Project Health
/// ─────────────────────────────────────────
///   my-saas    100  ████████████████████
///   api-core    72  ██████████████░░░░░░
/// ```
pub fn render_health_scores(scores: &[(String, u8)]) {
    if scores.is_empty() {
        return;
    }

    println!();
    println!("Project Health");
    println!("{}", "─".repeat(45));

    let name_width = scores.iter().map(|(n, _)| n.len()).max().unwrap_or(0).max(7);

    for (name, score) in scores {
        let filled = (*score as usize * 20) / 100;
        let empty = 20 - filled;
        let bar = format!("{}{}", "█".repeat(filled), "░".repeat(empty));
        println!("  {:<width$}  {:>3}  {}", name, score, bar, width = name_width);
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
