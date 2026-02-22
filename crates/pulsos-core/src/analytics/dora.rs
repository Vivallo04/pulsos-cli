use crate::domain::analytics::DoraMetrics;
use crate::domain::deployment::DeploymentStatus;
use crate::domain::project::CorrelatedEvent;
use std::time::Duration;

pub struct DoraCalculator;

impl DoraCalculator {
    /// Compute DORA metrics from a slice of correlated events.
    ///
    /// Includes all events that carry a production signal (branch == "main"/"master",
    /// deploy_target == "production", or environment_name == "production"). If no
    /// signal is set the event is included (cannot determine, assume production).
    /// Cross-platform confidence is not required — a Railway or Vercel success counts
    /// as a deployment even when there is no matching GitHub CI run.
    pub fn compute(events: &[CorrelatedEvent]) -> DoraMetrics {
        let valid: Vec<&CorrelatedEvent> =
            events.iter().filter(|e| Self::is_production(e)).collect();

        if valid.is_empty() {
            return DoraMetrics::default();
        }

        DoraMetrics {
            deployment_frequency: Self::deployment_frequency(&valid),
            lead_time_for_changes: Self::lead_time(&valid),
            change_failure_rate: Self::change_failure_rate(&valid),
            time_to_restore_service: Self::mttr(&valid),
            window_duration: Self::window_duration(&valid),
        }
    }

    fn is_production(e: &CorrelatedEvent) -> bool {
        let prod_branch = |ev: &crate::domain::deployment::DeploymentEvent| {
            ev.branch
                .as_deref()
                .map(|b| b == "main" || b == "master")
                .unwrap_or(false)
        };
        let prod_env = |ev: &crate::domain::deployment::DeploymentEvent| {
            ev.metadata.deploy_target.as_deref() == Some("production")
                || ev.metadata.environment_name.as_deref() == Some("production")
        };

        let any_signal = e.github.as_ref().map(&prod_branch).unwrap_or(false)
            || e.vercel
                .as_ref()
                .map(|v| prod_branch(v) || prod_env(v))
                .unwrap_or(false)
            || e.railway.as_ref().map(prod_env).unwrap_or(false);

        let has_signal = e
            .github
            .as_ref()
            .map(|g| g.branch.is_some())
            .unwrap_or(false)
            || e.vercel
                .as_ref()
                .map(|v| v.branch.is_some() || v.metadata.deploy_target.is_some())
                .unwrap_or(false)
            || e.railway
                .as_ref()
                .map(|r| r.metadata.environment_name.is_some())
                .unwrap_or(false);

        !has_signal || any_signal
    }

    fn deployment_frequency(valid: &[&CorrelatedEvent]) -> u32 {
        valid
            .iter()
            .filter(|e| {
                e.vercel
                    .as_ref()
                    .map(|v| v.status == DeploymentStatus::Success)
                    .or_else(|| {
                        e.railway
                            .as_ref()
                            .map(|r| r.status == DeploymentStatus::Success)
                    })
                    .unwrap_or(false)
            })
            .count() as u32
    }

    fn lead_time(valid: &[&CorrelatedEvent]) -> Option<Duration> {
        let mut total_secs = 0u64;
        let mut count = 0u32;

        for e in valid {
            let Some(gh) = &e.github else { continue };
            let cd = e.vercel.as_ref().or(e.railway.as_ref());
            let Some(cd) = cd else { continue };
            let end = cd.updated_at.unwrap_or(cd.created_at);
            let diff = (end - gh.created_at).num_seconds();
            if diff > 0 {
                total_secs += diff as u64;
                count += 1;
            }
        }

        (count > 0).then(|| Duration::from_secs(total_secs / count as u64))
    }

    fn change_failure_rate(valid: &[&CorrelatedEvent]) -> f64 {
        if valid.is_empty() {
            return 0.0;
        }
        let failed = valid
            .iter()
            .filter(|e| {
                e.vercel
                    .as_ref()
                    .map(|v| v.status == DeploymentStatus::Failed)
                    .or_else(|| {
                        e.railway
                            .as_ref()
                            .map(|r| r.status == DeploymentStatus::Failed)
                    })
                    .unwrap_or(false)
            })
            .count() as f64;
        (failed / valid.len() as f64) * 100.0
    }

    /// MTTR: for each failed→success pair (sorted by timestamp), measure the gap.
    fn mttr(valid: &[&CorrelatedEvent]) -> Option<Duration> {
        let mut sorted: Vec<&CorrelatedEvent> = valid.to_vec();
        sorted.sort_by_key(|e| e.timestamp);

        let cd_status = |e: &CorrelatedEvent| -> Option<DeploymentStatus> {
            e.vercel
                .as_ref()
                .map(|v| v.status.clone())
                .or_else(|| e.railway.as_ref().map(|r| r.status.clone()))
        };

        let mut total_secs = 0i64;
        let mut count = 0u32;
        let mut failure_time = None::<chrono::DateTime<chrono::Utc>>;

        for e in &sorted {
            match cd_status(e) {
                Some(DeploymentStatus::Failed) => {
                    failure_time = Some(e.timestamp);
                }
                Some(DeploymentStatus::Success) => {
                    if let Some(ft) = failure_time.take() {
                        let gap = (e.timestamp - ft).num_seconds();
                        if gap > 0 {
                            total_secs += gap;
                            count += 1;
                        }
                    }
                }
                _ => {}
            }
        }

        (count > 0).then(|| Duration::from_secs((total_secs / count as i64) as u64))
    }

    fn window_duration(valid: &[&CorrelatedEvent]) -> Option<Duration> {
        let min = valid.iter().map(|e| e.timestamp).min()?;
        let max = valid.iter().map(|e| e.timestamp).max()?;
        let secs = (max - min).num_seconds().max(0) as u64;
        Some(Duration::from_secs(secs))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::deployment::{DeploymentEvent, DeploymentStatus, EventMetadata, Platform};
    use crate::domain::project::{Confidence, CorrelatedEvent};
    use chrono::{Duration as ChronoDuration, Utc};

    fn gh_event(created_secs_ago: i64) -> DeploymentEvent {
        let ts = Utc::now() - ChronoDuration::seconds(created_secs_ago);
        DeploymentEvent {
            id: format!("gh-{created_secs_ago}"),
            platform: Platform::GitHub,
            status: DeploymentStatus::Success,
            commit_sha: Some(format!("sha{created_secs_ago}")),
            branch: Some("main".into()),
            title: None,
            actor: None,
            created_at: ts,
            updated_at: None,
            duration_secs: None,
            url: None,
            metadata: EventMetadata::default(),
            is_from_cache: false,
        }
    }

    fn cd_event(platform: Platform, status: DeploymentStatus, ts_secs_ago: i64) -> DeploymentEvent {
        let ts = Utc::now() - ChronoDuration::seconds(ts_secs_ago);
        DeploymentEvent {
            id: format!("cd-{ts_secs_ago}"),
            platform,
            status,
            commit_sha: None,
            branch: None,
            title: None,
            actor: None,
            created_at: ts,
            updated_at: Some(ts),
            duration_secs: None,
            url: None,
            metadata: EventMetadata {
                environment_name: Some("production".into()),
                ..Default::default()
            },
            is_from_cache: false,
        }
    }

    fn correlated(
        confidence: Confidence,
        github: Option<DeploymentEvent>,
        railway: Option<DeploymentEvent>,
        vercel: Option<DeploymentEvent>,
        ts_secs_ago: i64,
    ) -> CorrelatedEvent {
        CorrelatedEvent {
            project_name: Some("test-project".into()),
            commit_sha: Some(format!("sha{ts_secs_ago}")),
            github,
            railway,
            vercel,
            confidence,
            timestamp: Utc::now() - ChronoDuration::seconds(ts_secs_ago),
            is_stale: false,
        }
    }

    #[test]
    fn empty_events_returns_default() {
        let result = DoraCalculator::compute(&[]);
        assert_eq!(result.deployment_frequency, 0);
        assert!(result.lead_time_for_changes.is_none());
        assert_eq!(result.change_failure_rate, 0.0);
        assert!(result.time_to_restore_service.is_none());
    }

    #[test]
    fn deployment_frequency_counts_successes() {
        let events = vec![
            correlated(
                Confidence::High,
                Some(gh_event(120)),
                Some(cd_event(Platform::Railway, DeploymentStatus::Success, 60)),
                None,
                120,
            ),
            correlated(
                Confidence::High,
                Some(gh_event(240)),
                Some(cd_event(Platform::Railway, DeploymentStatus::Success, 180)),
                None,
                240,
            ),
            correlated(
                Confidence::High,
                Some(gh_event(360)),
                Some(cd_event(Platform::Railway, DeploymentStatus::Failed, 300)),
                None,
                360,
            ),
        ];

        let result = DoraCalculator::compute(&events);
        assert_eq!(result.deployment_frequency, 2);
    }

    #[test]
    fn lead_time_averages_correctly() {
        // Event 1: GH created 200s ago, CD updated 100s ago → lead time = 100s
        // Event 2: GH created 400s ago, CD updated 200s ago → lead time = 200s
        // Average = 150s
        let e1 = {
            let gh = gh_event(200);
            let mut cd = cd_event(Platform::Railway, DeploymentStatus::Success, 100);
            cd.updated_at = Some(Utc::now() - ChronoDuration::seconds(100));
            correlated(Confidence::High, Some(gh), Some(cd), None, 200)
        };
        let e2 = {
            let gh = gh_event(400);
            let mut cd = cd_event(Platform::Railway, DeploymentStatus::Success, 200);
            cd.updated_at = Some(Utc::now() - ChronoDuration::seconds(200));
            correlated(Confidence::High, Some(gh), Some(cd), None, 400)
        };

        let result = DoraCalculator::compute(&[e1, e2]);
        let lt = result.lead_time_for_changes.expect("should have lead time");
        // Allow ±5s tolerance for test timing
        assert!(
            lt.as_secs() >= 145 && lt.as_secs() <= 155,
            "Expected ~150s, got {}s",
            lt.as_secs()
        );
    }

    #[test]
    fn change_failure_rate_percentage() {
        // 1 failed out of 4 total = 25%
        let events: Vec<CorrelatedEvent> = vec![
            correlated(
                Confidence::High,
                None,
                Some(cd_event(Platform::Railway, DeploymentStatus::Failed, 400)),
                None,
                400,
            ),
            correlated(
                Confidence::High,
                None,
                Some(cd_event(Platform::Railway, DeploymentStatus::Success, 300)),
                None,
                300,
            ),
            correlated(
                Confidence::High,
                None,
                Some(cd_event(Platform::Railway, DeploymentStatus::Success, 200)),
                None,
                200,
            ),
            correlated(
                Confidence::High,
                None,
                Some(cd_event(Platform::Railway, DeploymentStatus::Success, 100)),
                None,
                100,
            ),
        ];

        let result = DoraCalculator::compute(&events);
        assert!(
            (result.change_failure_rate - 25.0).abs() < 0.01,
            "Expected 25.0%, got {}%",
            result.change_failure_rate
        );
    }

    #[test]
    fn mttr_measures_failure_to_restore_gap() {
        // Failed 3600s ago, restored 0s ago → MTTR ≈ 3600s (1h)
        let failed = correlated(
            Confidence::High,
            None,
            Some(cd_event(Platform::Railway, DeploymentStatus::Failed, 3600)),
            None,
            3600,
        );
        let restored = correlated(
            Confidence::High,
            None,
            Some(cd_event(Platform::Railway, DeploymentStatus::Success, 0)),
            None,
            0,
        );

        let result = DoraCalculator::compute(&[failed, restored]);
        let mttr = result.time_to_restore_service.expect("should have MTTR");
        // Allow ±5s tolerance
        assert!(
            mttr.as_secs() >= 3595 && mttr.as_secs() <= 3605,
            "Expected ~3600s, got {}s",
            mttr.as_secs()
        );
    }

    #[test]
    fn all_confidence_levels_included() {
        // Low and Unmatched events (Railway-only, no GitHub CI) are still production
        // deployments — they count toward deployment frequency and CFR.
        // Lead time requires a paired GitHub CI run, so it stays None here.
        let events = vec![
            correlated(
                Confidence::Low,
                None,
                Some(cd_event(Platform::Railway, DeploymentStatus::Success, 60)),
                None,
                60,
            ),
            correlated(
                Confidence::Unmatched,
                None,
                Some(cd_event(Platform::Railway, DeploymentStatus::Success, 120)),
                None,
                120,
            ),
        ];

        let result = DoraCalculator::compute(&events);
        // Both Railway-success events count as deployments
        assert_eq!(result.deployment_frequency, 2);
        // No GitHub CI paired — lead time cannot be computed
        assert!(result.lead_time_for_changes.is_none());
    }
}
