use super::deployment::{DeploymentStatus, Platform};
use crate::config::types::CorrelationConfig;
use crate::correlation::event_matches_project;
use crate::domain::deployment::DeploymentEvent;
use serde::{Deserialize, Serialize};

/// Per-platform breakdown of a health score.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct HealthBreakdown {
    pub total: u8,
    pub github_score: Option<u8>,
    pub railway_score: Option<u8>,
    pub vercel_score: Option<u8>,
    /// Effective weight after redistribution (0-100).
    pub github_weight: u8,
    pub railway_weight: u8,
    pub vercel_weight: u8,
}

/// Health score computation for a unified project.
///
/// Score: 0–100
/// Weights:
///   GitHub CI success rate (last 10 runs): 40%
///   Railway latest deployment status:      35%
///   Vercel latest deployment status:       25%
///
/// If a platform is not connected, its weight is redistributed
/// proportionally to the connected platforms.
pub struct HealthCalculator;

impl HealthCalculator {
    pub fn compute(
        github_runs: &[DeploymentStatus],
        railway_status: Option<DeploymentStatus>,
        vercel_status: Option<DeploymentStatus>,
    ) -> u8 {
        Self::compute_with_breakdown(github_runs, railway_status, vercel_status).total
    }

    pub fn compute_with_breakdown(
        github_runs: &[DeploymentStatus],
        railway_status: Option<DeploymentStatus>,
        vercel_status: Option<DeploymentStatus>,
    ) -> HealthBreakdown {
        let mut total_weight = 0.0_f64;
        let mut weighted_score = 0.0_f64;

        let mut breakdown = HealthBreakdown::default();

        // GitHub: success rate of last N runs
        let github_raw = if !github_runs.is_empty() {
            let success_count = github_runs
                .iter()
                .filter(|s| matches!(s, DeploymentStatus::Success))
                .count();
            let rate = success_count as f64 / github_runs.len() as f64;
            weighted_score += rate * 40.0;
            total_weight += 40.0;
            Some((rate * 100.0).round() as u8)
        } else {
            None
        };

        // Railway: binary — latest deployment status
        let railway_raw = if let Some(status) = railway_status {
            let score = Self::status_score(status);
            weighted_score += score * 35.0;
            total_weight += 35.0;
            Some((score * 100.0).round() as u8)
        } else {
            None
        };

        // Vercel: binary — latest deployment status
        let vercel_raw = if let Some(status) = vercel_status {
            let score = Self::status_score(status);
            weighted_score += score * 25.0;
            total_weight += 25.0;
            Some((score * 100.0).round() as u8)
        } else {
            None
        };

        if total_weight == 0.0 {
            return breakdown;
        }

        // Compute redistributed weights (0-100 scale).
        breakdown.github_weight = if github_raw.is_some() {
            (40.0 / total_weight * 100.0).round() as u8
        } else {
            0
        };
        breakdown.railway_weight = if railway_raw.is_some() {
            (35.0 / total_weight * 100.0).round() as u8
        } else {
            0
        };
        breakdown.vercel_weight = if vercel_raw.is_some() {
            (25.0 / total_weight * 100.0).round() as u8
        } else {
            0
        };

        breakdown.github_score = github_raw;
        breakdown.railway_score = railway_raw;
        breakdown.vercel_score = vercel_raw;
        breakdown.total = ((weighted_score / total_weight) * 100.0).round() as u8;

        breakdown
    }

    fn status_score(status: DeploymentStatus) -> f64 {
        match status {
            DeploymentStatus::Success => 1.0,
            DeploymentStatus::InProgress | DeploymentStatus::Queued => 0.7,
            DeploymentStatus::Sleeping => 0.5,
            DeploymentStatus::Skipped | DeploymentStatus::Cancelled => 0.5,
            DeploymentStatus::ActionRequired => 0.3,
            DeploymentStatus::Failed => 0.0,
            DeploymentStatus::Unknown(_) => 0.5,
        }
    }
}

/// Compute per-project health scores using correlation configs and fetched events.
///
/// For each correlation, filters events by project match and delegates to
/// `HealthCalculator::compute`.
pub fn compute_project_health_scores(
    correlations: &[CorrelationConfig],
    events: &[DeploymentEvent],
) -> Vec<(String, u8)> {
    let mut scores = Vec::new();

    for corr in correlations {
        let github_runs: Vec<DeploymentStatus> = if corr.github_repo.is_some() {
            events
                .iter()
                .filter(|e| {
                    e.platform == Platform::GitHub
                        && e.metadata.workflow_name.is_some()
                        && event_matches_project(e, corr)
                })
                .take(10)
                .map(|e| e.status.clone())
                .collect()
        } else {
            Vec::new()
        };

        let railway_status = if corr.railway_project.is_some() {
            events
                .iter()
                .filter(|e| e.platform == Platform::Railway && event_matches_project(e, corr))
                .max_by_key(|e| e.created_at)
                .map(|e| e.status.clone())
        } else {
            None
        };

        let vercel_status = if corr.vercel_project.is_some() {
            events
                .iter()
                .filter(|e| e.platform == Platform::Vercel && event_matches_project(e, corr))
                .max_by_key(|e| e.created_at)
                .map(|e| e.status.clone())
        } else {
            None
        };

        let score = HealthCalculator::compute(&github_runs, railway_status, vercel_status);
        scores.push((corr.name.clone(), score));
    }

    scores
}

/// Compute per-project health breakdowns using correlation configs and fetched events.
///
/// Same logic as `compute_project_health_scores` but returns `HealthBreakdown` with
/// per-platform scores and redistributed weights.
pub fn compute_project_health_breakdowns(
    correlations: &[CorrelationConfig],
    events: &[DeploymentEvent],
) -> Vec<(String, HealthBreakdown)> {
    let mut breakdowns = Vec::new();

    for corr in correlations {
        let github_runs: Vec<DeploymentStatus> = if corr.github_repo.is_some() {
            events
                .iter()
                .filter(|e| {
                    e.platform == Platform::GitHub
                        && e.metadata.workflow_name.is_some()
                        && event_matches_project(e, corr)
                })
                .take(10)
                .map(|e| e.status.clone())
                .collect()
        } else {
            Vec::new()
        };

        let railway_status = if corr.railway_project.is_some() {
            events
                .iter()
                .filter(|e| e.platform == Platform::Railway && event_matches_project(e, corr))
                .max_by_key(|e| e.created_at)
                .map(|e| e.status.clone())
        } else {
            None
        };

        let vercel_status = if corr.vercel_project.is_some() {
            events
                .iter()
                .filter(|e| e.platform == Platform::Vercel && event_matches_project(e, corr))
                .max_by_key(|e| e.created_at)
                .map(|e| e.status.clone())
        } else {
            None
        };

        let breakdown =
            HealthCalculator::compute_with_breakdown(&github_runs, railway_status, vercel_status);
        breakdowns.push((corr.name.clone(), breakdown));
    }

    breakdowns
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_platforms_success() {
        let github = vec![DeploymentStatus::Success; 10];
        let score = HealthCalculator::compute(
            &github,
            Some(DeploymentStatus::Success),
            Some(DeploymentStatus::Success),
        );
        assert_eq!(score, 100);
    }

    #[test]
    fn all_platforms_failed() {
        let github = vec![DeploymentStatus::Failed; 10];
        let score = HealthCalculator::compute(
            &github,
            Some(DeploymentStatus::Failed),
            Some(DeploymentStatus::Failed),
        );
        assert_eq!(score, 0);
    }

    #[test]
    fn mixed_github_runs() {
        // 7 success, 3 failure = 70% success rate
        let mut github = vec![DeploymentStatus::Success; 7];
        github.extend(vec![DeploymentStatus::Failed; 3]);
        let score = HealthCalculator::compute(
            &github,
            Some(DeploymentStatus::Success),
            Some(DeploymentStatus::Success),
        );
        // GitHub: 0.7 * 40 = 28, Railway: 1.0 * 35 = 35, Vercel: 1.0 * 25 = 25
        // Total: 88 / 100 = 88
        assert_eq!(score, 88);
    }

    #[test]
    fn github_only() {
        let github = vec![DeploymentStatus::Success; 10];
        let score = HealthCalculator::compute(&github, None, None);
        // 40/40 * 100 = 100
        assert_eq!(score, 100);
    }

    #[test]
    fn railway_only() {
        let score = HealthCalculator::compute(&[], Some(DeploymentStatus::Success), None);
        assert_eq!(score, 100);
    }

    #[test]
    fn no_platforms() {
        let score = HealthCalculator::compute(&[], None, None);
        assert_eq!(score, 0);
    }

    #[test]
    fn in_progress_scores() {
        let score = HealthCalculator::compute(&[], Some(DeploymentStatus::InProgress), None);
        // 0.7 * 35 / 35 * 100 = 70
        assert_eq!(score, 70);
    }

    #[test]
    fn sleeping_scores() {
        let score = HealthCalculator::compute(&[], Some(DeploymentStatus::Sleeping), None);
        // 0.5 * 35 / 35 * 100 = 50
        assert_eq!(score, 50);
    }

    #[test]
    fn breakdown_all_platforms() {
        let github = vec![DeploymentStatus::Success; 10];
        let bd = HealthCalculator::compute_with_breakdown(
            &github,
            Some(DeploymentStatus::Success),
            Some(DeploymentStatus::Success),
        );
        assert_eq!(bd.total, 100);
        assert_eq!(bd.github_score, Some(100));
        assert_eq!(bd.railway_score, Some(100));
        assert_eq!(bd.vercel_score, Some(100));
        assert_eq!(bd.github_weight, 40);
        assert_eq!(bd.railway_weight, 35);
        assert_eq!(bd.vercel_weight, 25);
    }

    #[test]
    fn breakdown_github_only_redistributes_weights() {
        let github = vec![DeploymentStatus::Success; 10];
        let bd = HealthCalculator::compute_with_breakdown(&github, None, None);
        assert_eq!(bd.total, 100);
        assert_eq!(bd.github_score, Some(100));
        assert_eq!(bd.railway_score, None);
        assert_eq!(bd.vercel_score, None);
        assert_eq!(bd.github_weight, 100);
        assert_eq!(bd.railway_weight, 0);
        assert_eq!(bd.vercel_weight, 0);
    }

    #[test]
    fn breakdown_no_platforms() {
        let bd = HealthCalculator::compute_with_breakdown(&[], None, None);
        assert_eq!(bd.total, 0);
        assert_eq!(bd.github_weight, 0);
        assert_eq!(bd.railway_weight, 0);
        assert_eq!(bd.vercel_weight, 0);
    }

    #[test]
    fn breakdown_mixed_scores() {
        // 7/10 GitHub success = 70%, Railway success = 100%
        let mut github = vec![DeploymentStatus::Success; 7];
        github.extend(vec![DeploymentStatus::Failed; 3]);
        let bd = HealthCalculator::compute_with_breakdown(
            &github,
            Some(DeploymentStatus::Success),
            None,
        );
        assert_eq!(bd.github_score, Some(70));
        assert_eq!(bd.railway_score, Some(100));
        assert_eq!(bd.vercel_score, None);
        // Weights redistributed: 40/(40+35)=53%, 35/(40+35)=47%
        assert_eq!(bd.github_weight, 53);
        assert_eq!(bd.railway_weight, 47);
        assert_eq!(bd.vercel_weight, 0);
    }
}
