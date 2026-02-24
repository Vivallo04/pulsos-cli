//! Desktop notifications — fires OS toast alerts when endpoint health transitions
//! or CI / deployment status changes.

use std::collections::HashMap;

use pulsos_core::domain::deployment::{DeploymentEvent, DeploymentStatus, Platform};

use crate::tui::app::DataSnapshot;
use notify_rust::Notification;

pub struct NotificationState {
    prev_snapshot: Option<DataSnapshot>,
}

impl NotificationState {
    pub fn new() -> Self {
        Self {
            prev_snapshot: None,
        }
    }

    /// Compare `current` against the previous snapshot and fire notifications
    /// for endpoint state transitions (up→down or down→up) and CI / deployment
    /// status transitions (queued→running, running→success/failed).
    pub fn check_and_notify(&mut self, current: &DataSnapshot) {
        if let Some(prev) = &self.prev_snapshot {
            // --- Endpoint health notifications ---
            for (name, tel) in &current.telemetry {
                let cur_up = tel.latest_ping().map(|p| p.is_up).unwrap_or(true);
                let prev_up = prev
                    .telemetry
                    .get(name)
                    .and_then(|t| t.latest_ping())
                    .map(|p| p.is_up)
                    .unwrap_or(true);

                if prev_up && !cur_up {
                    let url = tel
                        .latest_ping()
                        .map(|p| p.url.as_str())
                        .unwrap_or(name.as_str());
                    send_notification("API Offline", &format!("{url} is unreachable"));
                } else if !prev_up && cur_up {
                    send_notification("API Restored", &format!("{name} is back online"));
                }
            }

            // --- CI / deployment status notifications ---
            // Build a map of event ID → status from the previous snapshot.
            // New events (not present in prev) are skipped to avoid flooding
            // on daemon restart with notifications for already-completed runs.
            let prev_statuses: HashMap<&str, &DeploymentStatus> = prev
                .events
                .iter()
                .map(|e| (e.id.as_str(), &e.status))
                .collect();

            for event in &current.events {
                let cur_s = &event.status;
                let Some(prev_s) = prev_statuses.get(event.id.as_str()).copied() else {
                    continue; // new event — establish baseline silently
                };
                if prev_s == cur_s {
                    continue; // no change
                }

                match (prev_s, cur_s) {
                    // Workflow / deployment started
                    (DeploymentStatus::Queued, DeploymentStatus::InProgress) => {
                        let (title, body) = notification_text(event, "running");
                        send_notification(&title, &body);
                    }
                    // Succeeded from an active state
                    (
                        DeploymentStatus::InProgress | DeploymentStatus::Queued,
                        DeploymentStatus::Success,
                    ) => {
                        let (title, body) = notification_text(event, "succeeded");
                        send_notification(&title, &body);
                    }
                    // Failed from an active state
                    (
                        DeploymentStatus::InProgress | DeploymentStatus::Queued,
                        DeploymentStatus::Failed,
                    ) => {
                        let (title, body) = notification_text(event, "failed");
                        send_notification(&title, &body);
                    }
                    _ => {}
                }
            }
        }
        self.prev_snapshot = Some(current.clone());
    }
}

/// Build a (title, body) pair for a deployment notification.
///
/// * GitHub events use the workflow name and branch.
/// * Railway events use the service name and environment.
/// * Vercel events use the deployment title and deploy target.
fn notification_text(event: &DeploymentEvent, verb: &str) -> (String, String) {
    let branch = event.branch.as_deref().unwrap_or("unknown");

    let (name, body) = match event.platform {
        Platform::GitHub => {
            let wf = event
                .metadata
                .workflow_name
                .as_deref()
                .or(event.title.as_deref())
                .unwrap_or("CI");
            (wf.to_string(), format!("branch: {branch}"))
        }
        Platform::Railway => {
            let svc = event
                .metadata
                .service_name
                .as_deref()
                .or(event.title.as_deref())
                .unwrap_or("deployment");
            let env = event.metadata.environment_name.as_deref().unwrap_or("");
            let extra = if env.is_empty() {
                String::new()
            } else {
                format!("env: {env}")
            };
            (svc.to_string(), extra)
        }
        Platform::Vercel => {
            let svc = event.title.as_deref().unwrap_or("Vercel deployment");
            let target = event
                .metadata
                .deploy_target
                .as_deref()
                .unwrap_or("preview");
            (svc.to_string(), format!("target: {target}"))
        }
    };

    (format!("{name} {verb}"), body)
}

fn send_notification(title: &str, body: &str) {
    // Never panic if notifications fail (e.g. no notification daemon on Linux).
    Notification::new().summary(title).body(body).show().ok();
}
