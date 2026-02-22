//! Desktop notifications — fires OS toast alerts when endpoint health transitions.

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
    /// for any endpoint state transitions (up→down or down→up).
    pub fn check_and_notify(&mut self, current: &DataSnapshot) {
        if let Some(prev) = &self.prev_snapshot {
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
        }
        self.prev_snapshot = Some(current.clone());
    }
}

fn send_notification(title: &str, body: &str) {
    // Never panic if notifications fail (e.g. no notification daemon on Linux).
    Notification::new().summary(title).body(body).show().ok();
}
