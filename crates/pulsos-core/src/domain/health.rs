use super::deployment::DeploymentStatus;

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
        let mut total_weight = 0.0_f64;
        let mut weighted_score = 0.0_f64;

        // GitHub: success rate of last N runs
        if !github_runs.is_empty() {
            let success_count = github_runs
                .iter()
                .filter(|s| matches!(s, DeploymentStatus::Success))
                .count();
            let rate = success_count as f64 / github_runs.len() as f64;
            weighted_score += rate * 40.0;
            total_weight += 40.0;
        }

        // Railway: binary — latest deployment status
        if let Some(status) = railway_status {
            let score = Self::status_score(status);
            weighted_score += score * 35.0;
            total_weight += 35.0;
        }

        // Vercel: binary — latest deployment status
        if let Some(status) = vercel_status {
            let score = Self::status_score(status);
            weighted_score += score * 25.0;
            total_weight += 25.0;
        }

        if total_weight == 0.0 {
            return 0;
        }

        // Normalize to 0–100
        ((weighted_score / total_weight) * 100.0).round() as u8
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
}
