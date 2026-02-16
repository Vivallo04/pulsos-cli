use chrono::Utc;
use pulsos_core::domain::deployment::{DeploymentEvent, DeploymentStatus};
use tabled::settings::Style;
use tabled::{Table, Tabled};

#[derive(Tabled)]
struct EventRow {
    #[tabled(rename = "Status")]
    status: String,
    #[tabled(rename = "Platform")]
    platform: String,
    #[tabled(rename = "Title")]
    title: String,
    #[tabled(rename = "Branch")]
    branch: String,
    #[tabled(rename = "Actor")]
    actor: String,
    #[tabled(rename = "Age")]
    age: String,
    #[tabled(rename = "Duration")]
    duration: String,
}

pub fn render(events: &[DeploymentEvent], _no_color: bool) {
    if events.is_empty() {
        println!("No deployment events found.");
        return;
    }

    let rows: Vec<EventRow> = events
        .iter()
        .map(|e| EventRow {
            status: status_indicator(&e.status),
            platform: e.platform.to_string(),
            title: e
                .title
                .clone()
                .unwrap_or_else(|| e.id.chars().take(12).collect()),
            branch: e.branch.clone().unwrap_or_else(|| "-".into()),
            actor: e.actor.clone().unwrap_or_else(|| "-".into()),
            age: format_age(e.created_at),
            duration: e
                .duration_secs
                .map(format_duration)
                .unwrap_or_else(|| "-".into()),
        })
        .collect();

    let table = Table::new(&rows).with(Style::rounded()).to_string();

    println!("{table}");
}

fn status_indicator(status: &DeploymentStatus) -> String {
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

fn format_age(created_at: chrono::DateTime<Utc>) -> String {
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

fn format_duration(secs: u64) -> String {
    if secs < 60 {
        format!("{secs}s")
    } else if secs < 3600 {
        format!("{}m {}s", secs / 60, secs % 60)
    } else {
        format!("{}h {}m", secs / 3600, (secs % 3600) / 60)
    }
}
