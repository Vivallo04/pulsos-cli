//! Timestamp-based heuristic matching for event correlation.
//!
//! Used primarily for Railway events which often lack commit SHAs.
//! Matches events that occurred within a configurable time window.

use crate::domain::deployment::DeploymentEvent;

/// Default timestamp matching window in seconds (from PRD Section 3.3.1).
pub const TIMESTAMP_WINDOW_SECS: i64 = 120;

/// Result of a heuristic timestamp match.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeuristicMatch {
    /// Index into the candidates array.
    pub candidate_index: usize,
    /// Absolute time difference in seconds.
    pub time_diff_secs: i64,
}

/// Find the closest candidate event to a reference event by timestamp.
///
/// Returns the candidate within `window_secs` that has the smallest time
/// difference — not just the first match. Skips candidates that are already
/// claimed.
pub fn find_closest_by_timestamp(
    reference: &DeploymentEvent,
    candidates: &[&DeploymentEvent],
    claimed: &[bool],
    window_secs: i64,
) -> Option<HeuristicMatch> {
    let ref_ts = reference.created_at.timestamp();
    let mut best: Option<HeuristicMatch> = None;

    for (i, candidate) in candidates.iter().enumerate() {
        if claimed[i] {
            continue;
        }

        let diff = (candidate.created_at.timestamp() - ref_ts).abs();
        if diff > window_secs {
            continue;
        }

        match &best {
            Some(current_best) if diff >= current_best.time_diff_secs => {}
            _ => {
                best = Some(HeuristicMatch {
                    candidate_index: i,
                    time_diff_secs: diff,
                });
            }
        }
    }

    best
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::deployment::{DeploymentStatus, EventMetadata, Platform};
    use chrono::{Duration, Utc};

    fn make_event_at(platform: Platform, offset_secs: i64) -> DeploymentEvent {
        DeploymentEvent {
            id: format!("evt-{offset_secs}"),
            platform,
            status: DeploymentStatus::Success,
            commit_sha: None,
            branch: None,
            title: None,
            actor: None,
            created_at: Utc::now() + Duration::seconds(offset_secs),
            updated_at: None,
            duration_secs: None,
            url: None,
            metadata: EventMetadata::default(),
        }
    }

    #[test]
    fn within_window_matches() {
        let reference = make_event_at(Platform::GitHub, 0);
        let candidate = make_event_at(Platform::Railway, 60);
        let candidates = vec![&candidate];
        let claimed = vec![false];

        let result =
            find_closest_by_timestamp(&reference, &candidates, &claimed, TIMESTAMP_WINDOW_SECS);
        assert!(result.is_some());
        let m = result.unwrap();
        assert_eq!(m.candidate_index, 0);
        assert_eq!(m.time_diff_secs, 60);
    }

    #[test]
    fn outside_window_no_match() {
        let reference = make_event_at(Platform::GitHub, 0);
        let candidate = make_event_at(Platform::Railway, 200);
        let candidates = vec![&candidate];
        let claimed = vec![false];

        let result =
            find_closest_by_timestamp(&reference, &candidates, &claimed, TIMESTAMP_WINDOW_SECS);
        assert!(result.is_none());
    }

    #[test]
    fn closest_wins() {
        let reference = make_event_at(Platform::GitHub, 0);
        let far = make_event_at(Platform::Railway, 100);
        let close = make_event_at(Platform::Railway, 30);
        let candidates = vec![&far, &close];
        let claimed = vec![false, false];

        let result =
            find_closest_by_timestamp(&reference, &candidates, &claimed, TIMESTAMP_WINDOW_SECS);
        assert!(result.is_some());
        let m = result.unwrap();
        assert_eq!(m.candidate_index, 1); // close is at index 1
        assert_eq!(m.time_diff_secs, 30);
    }

    #[test]
    fn claimed_candidate_skipped() {
        let reference = make_event_at(Platform::GitHub, 0);
        let c1 = make_event_at(Platform::Railway, 10);
        let c2 = make_event_at(Platform::Railway, 50);
        let candidates = vec![&c1, &c2];
        let claimed = vec![true, false]; // c1 is claimed

        let result =
            find_closest_by_timestamp(&reference, &candidates, &claimed, TIMESTAMP_WINDOW_SECS);
        assert!(result.is_some());
        let m = result.unwrap();
        assert_eq!(m.candidate_index, 1); // skipped c1, picked c2
        assert_eq!(m.time_diff_secs, 50);
    }

    #[test]
    fn boundary_exactly_at_window() {
        let reference = make_event_at(Platform::GitHub, 0);
        let candidate = make_event_at(Platform::Railway, TIMESTAMP_WINDOW_SECS);
        let candidates = vec![&candidate];
        let claimed = vec![false];

        let result =
            find_closest_by_timestamp(&reference, &candidates, &claimed, TIMESTAMP_WINDOW_SECS);
        // Exactly at the boundary should match (<=)
        assert!(result.is_some());
        assert_eq!(result.unwrap().time_diff_secs, TIMESTAMP_WINDOW_SECS);
    }
}
