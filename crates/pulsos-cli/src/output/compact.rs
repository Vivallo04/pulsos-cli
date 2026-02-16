use chrono::Utc;
use pulsos_core::domain::deployment::{DeploymentEvent, DeploymentStatus};

pub fn render(events: &[DeploymentEvent]) {
    if events.is_empty() {
        println!("No deployment events found.");
        return;
    }

    for event in events {
        let status = match event.status {
            DeploymentStatus::Success => "OK",
            DeploymentStatus::Failed => "FAIL",
            DeploymentStatus::InProgress => "RUN",
            DeploymentStatus::Queued => "QUE",
            DeploymentStatus::Cancelled => "CAN",
            DeploymentStatus::Skipped => "SKP",
            DeploymentStatus::ActionRequired => "ACT",
            DeploymentStatus::Sleeping => "SLP",
            DeploymentStatus::Unknown(_) => "???",
        };

        let age = format_age_compact(event.created_at);
        let title = event
            .title
            .clone()
            .unwrap_or_else(|| event.id.chars().take(12).collect::<String>());
        let platform = &event.platform;
        let branch = event.branch.as_deref().unwrap_or("-");

        println!("[{status:>4}] {platform} | {title} ({branch}) {age}");
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
