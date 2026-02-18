//! SHA-based matching for GitHub <-> Vercel event correlation.

use crate::domain::deployment::DeploymentEvent;

/// Find events that share the same commit SHA across two platform groups.
///
/// Returns index pairs `(a_idx, b_idx)` for matched events.
/// Uses a `claimed_b` array to prevent double-claiming on the `b` side.
pub fn find_sha_matches(
    a_events: &[&DeploymentEvent],
    b_events: &[&DeploymentEvent],
) -> Vec<(usize, usize)> {
    let mut claimed_b = vec![false; b_events.len()];
    let mut matches = Vec::new();

    for (ai, a_event) in a_events.iter().enumerate() {
        let a_sha = match a_event.commit_sha.as_deref() {
            Some(sha) if !sha.is_empty() => sha,
            _ => continue,
        };

        for (bi, b_event) in b_events.iter().enumerate() {
            if claimed_b[bi] {
                continue;
            }

            let b_sha = match b_event.commit_sha.as_deref() {
                Some(sha) if !sha.is_empty() => sha,
                _ => continue,
            };

            if a_sha == b_sha {
                matches.push((ai, bi));
                claimed_b[bi] = true;
                break;
            }
        }
    }

    matches
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::deployment::{DeploymentStatus, EventMetadata, Platform};
    use chrono::Utc;

    fn make_event(platform: Platform, sha: Option<&str>) -> DeploymentEvent {
        DeploymentEvent {
            id: format!("evt-{}", sha.unwrap_or("none")),
            platform,
            status: DeploymentStatus::Success,
            commit_sha: sha.map(Into::into),
            branch: Some("main".into()),
            title: None,
            actor: None,
            created_at: Utc::now(),
            updated_at: None,
            duration_secs: None,
            url: None,
            metadata: EventMetadata::default(),
        }
    }

    #[test]
    fn matching_sha_pairs() {
        let gh = [
            make_event(Platform::GitHub, Some("abc123")),
            make_event(Platform::GitHub, Some("def456")),
        ];
        let vc = [
            make_event(Platform::Vercel, Some("def456")),
            make_event(Platform::Vercel, Some("abc123")),
        ];

        let gh_refs: Vec<&DeploymentEvent> = gh.iter().collect();
        let vc_refs: Vec<&DeploymentEvent> = vc.iter().collect();

        let matches = find_sha_matches(&gh_refs, &vc_refs);
        assert_eq!(matches.len(), 2);
        // abc123: gh[0] -> vc[1], def456: gh[1] -> vc[0]
        assert_eq!(matches[0], (0, 1));
        assert_eq!(matches[1], (1, 0));
    }

    #[test]
    fn no_match_different_shas() {
        let gh = [make_event(Platform::GitHub, Some("abc123"))];
        let vc = [make_event(Platform::Vercel, Some("xyz789"))];

        let gh_refs: Vec<&DeploymentEvent> = gh.iter().collect();
        let vc_refs: Vec<&DeploymentEvent> = vc.iter().collect();

        let matches = find_sha_matches(&gh_refs, &vc_refs);
        assert!(matches.is_empty());
    }

    #[test]
    fn prevents_double_claiming() {
        let gh = [
            make_event(Platform::GitHub, Some("abc123")),
            make_event(Platform::GitHub, Some("abc123")),
        ];
        let vc = [make_event(Platform::Vercel, Some("abc123"))];

        let gh_refs: Vec<&DeploymentEvent> = gh.iter().collect();
        let vc_refs: Vec<&DeploymentEvent> = vc.iter().collect();

        let matches = find_sha_matches(&gh_refs, &vc_refs);
        // Only the first GitHub event should claim the single Vercel event
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0], (0, 0));
    }

    #[test]
    fn none_sha_skipped() {
        let gh = [make_event(Platform::GitHub, None)];
        let vc = [make_event(Platform::Vercel, Some("abc123"))];

        let gh_refs: Vec<&DeploymentEvent> = gh.iter().collect();
        let vc_refs: Vec<&DeploymentEvent> = vc.iter().collect();

        let matches = find_sha_matches(&gh_refs, &vc_refs);
        assert!(matches.is_empty());
    }

    #[test]
    fn empty_inputs() {
        let empty: Vec<&DeploymentEvent> = vec![];
        let gh = [make_event(Platform::GitHub, Some("abc123"))];
        let gh_refs: Vec<&DeploymentEvent> = gh.iter().collect();

        assert!(find_sha_matches(&empty, &empty).is_empty());
        assert!(find_sha_matches(&gh_refs, &empty).is_empty());
        assert!(find_sha_matches(&empty, &gh_refs).is_empty());
    }
}
