//! Builds correlation candidates from discovered resources across platforms.
//!
//! Two-tier matching:
//! 1. Vercel `link.repo` → exact GitHub repo match
//! 2. Name stem matching (strip common suffixes like `-web`, `-api`, etc.)

use crate::config::types::CorrelationConfig;
use crate::platform::DiscoveredResource;
use std::collections::HashMap;

/// A proposed correlation between resources across platforms.
#[derive(Debug, Clone)]
pub struct CorrelationCandidate {
    pub name: String,
    pub github: Option<DiscoveredResource>,
    pub railway: Option<DiscoveredResource>,
    pub vercel: Option<DiscoveredResource>,
    /// For Vercel projects, the linked GitHub repo (e.g., "myorg/my-saas").
    pub vercel_linked_repo: Option<String>,
    pub confidence: MatchConfidence,
}

/// How the correlation was established.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MatchConfidence {
    /// Vercel project.link.repo matches a GitHub repo exactly.
    LinkedRepo,
    /// Name stems match across platforms (e.g., "my-saas" in "my-saas-web").
    ExactStem,
    /// No match — standalone resource from a single platform.
    Unmatched,
}

/// Input for the correlation engine: selected resources grouped by platform.
pub struct DiscoveryResults {
    pub github: Vec<DiscoveredResource>,
    pub railway: Vec<DiscoveredResource>,
    /// Vercel resources paired with their linked GitHub repo (if any).
    pub vercel: Vec<(DiscoveredResource, Option<String>)>,
}

/// Build correlation candidates from discovered resources.
///
/// Algorithm:
/// 1. Index GitHub repos by full_name and name stem.
/// 2. For each Vercel project with a link.repo, match to GitHub by full_name.
/// 3. For remaining, match by name stem across all platforms.
/// 4. Leftovers become standalone correlations.
pub fn build_correlations(results: &DiscoveryResults) -> Vec<CorrelationCandidate> {
    let mut candidates: Vec<CorrelationCandidate> = Vec::new();

    // Track which resources have been claimed by a correlation.
    let mut claimed_github: Vec<bool> = vec![false; results.github.len()];
    let mut claimed_railway: Vec<bool> = vec![false; results.railway.len()];
    let mut claimed_vercel: Vec<bool> = vec![false; results.vercel.len()];

    // Index GitHub repos by full_name for Vercel link matching.
    let github_by_fullname: HashMap<&str, usize> = results
        .github
        .iter()
        .enumerate()
        .map(|(i, r)| (r.platform_id.as_str(), i))
        .collect();

    // ── Tier 1: Vercel link.repo → GitHub exact match ──

    for (vi, (vc_resource, linked_repo)) in results.vercel.iter().enumerate() {
        if let Some(repo_name) = linked_repo {
            if let Some(&gi) = github_by_fullname.get(repo_name.as_str()) {
                if !claimed_github[gi] {
                    let gh = &results.github[gi];
                    let name = gh.display_name.clone();

                    // Try to find a Railway match by stem too
                    let stem = name_stem(&name);
                    let railway_match = find_railway_by_stem(stem, results, &claimed_railway);

                    if let Some(ri) = railway_match {
                        claimed_railway[ri] = true;
                    }

                    candidates.push(CorrelationCandidate {
                        name,
                        github: Some(gh.clone()),
                        railway: railway_match.map(|ri| results.railway[ri].clone()),
                        vercel: Some(vc_resource.clone()),
                        vercel_linked_repo: Some(repo_name.clone()),
                        confidence: MatchConfidence::LinkedRepo,
                    });

                    claimed_github[gi] = true;
                    claimed_vercel[vi] = true;
                }
            }
        }
    }

    // ── Tier 2: Name stem matching ──

    // Build stem groups from unclaimed resources.
    let mut stem_groups: HashMap<String, StemGroup> = HashMap::new();

    for (gi, gh) in results.github.iter().enumerate() {
        if claimed_github[gi] {
            continue;
        }
        let stem = name_stem(&gh.display_name).to_string();
        stem_groups
            .entry(stem.clone())
            .or_insert_with(|| StemGroup::new(stem))
            .github_indices
            .push(gi);
    }

    for (ri, rw) in results.railway.iter().enumerate() {
        if claimed_railway[ri] {
            continue;
        }
        // Railway display_name is "project / service / env". Extract the project part.
        let project_name = rw
            .display_name
            .split(" / ")
            .next()
            .unwrap_or(&rw.display_name);
        let stem = name_stem(project_name).to_string();
        stem_groups
            .entry(stem.clone())
            .or_insert_with(|| StemGroup::new(stem))
            .railway_indices
            .push(ri);
    }

    for (vi, (vc_resource, _)) in results.vercel.iter().enumerate() {
        if claimed_vercel[vi] {
            continue;
        }
        let stem = name_stem(&vc_resource.display_name).to_string();
        stem_groups
            .entry(stem.clone())
            .or_insert_with(|| StemGroup::new(stem))
            .vercel_indices
            .push(vi);
    }

    // Convert stem groups with multiple platforms into correlations.
    let mut stems: Vec<_> = stem_groups.into_values().collect();
    stems.sort_by(|a, b| a.stem.cmp(&b.stem));

    for group in &stems {
        let has_multiple = ((!group.github_indices.is_empty()) as u8)
            + ((!group.railway_indices.is_empty()) as u8)
            + ((!group.vercel_indices.is_empty()) as u8)
            > 1;

        let confidence = if has_multiple {
            MatchConfidence::ExactStem
        } else {
            MatchConfidence::Unmatched
        };

        // Take the first resource from each platform for this stem.
        let gh = group
            .github_indices
            .first()
            .map(|&i| results.github[i].clone());
        let rw = group
            .railway_indices
            .first()
            .map(|&i| results.railway[i].clone());
        let vc = group
            .vercel_indices
            .first()
            .map(|&i| results.vercel[i].0.clone());

        // Mark all indices as claimed.
        for &i in &group.github_indices {
            claimed_github[i] = true;
        }
        for &i in &group.railway_indices {
            claimed_railway[i] = true;
        }
        for &i in &group.vercel_indices {
            claimed_vercel[i] = true;
        }

        candidates.push(CorrelationCandidate {
            name: group.stem.clone(),
            github: gh,
            railway: rw,
            vercel: vc,
            vercel_linked_repo: None,
            confidence,
        });
    }

    candidates
}

/// Convert a correlation candidate to a config entry.
pub fn candidate_to_config(candidate: &CorrelationCandidate) -> CorrelationConfig {
    CorrelationConfig {
        name: candidate.name.clone(),
        github_repo: candidate.github.as_ref().map(|r| r.platform_id.clone()),
        railway_project: candidate.railway.as_ref().map(|r| r.platform_id.clone()),
        railway_workspace: candidate.railway.as_ref().map(|r| r.group.clone()),
        railway_environment: None,
        vercel_project: candidate.vercel.as_ref().map(|r| r.platform_id.clone()),
        vercel_team: candidate.vercel.as_ref().map(|r| r.group.clone()),
        branch_mapping: HashMap::new(),
    }
}

// ── Internal helpers ──

struct StemGroup {
    stem: String,
    github_indices: Vec<usize>,
    railway_indices: Vec<usize>,
    vercel_indices: Vec<usize>,
}

impl StemGroup {
    fn new(stem: String) -> Self {
        Self {
            stem,
            github_indices: Vec::new(),
            railway_indices: Vec::new(),
            vercel_indices: Vec::new(),
        }
    }
}

/// Extract the name "stem" by stripping common deployment suffixes.
///
/// Examples:
/// - "my-saas-web" → "my-saas"
/// - "my-saas-api" → "my-saas"
/// - "my-saas" → "my-saas"
/// - "api-core" → "api-core" (no known suffix)
pub fn name_stem(name: &str) -> &str {
    const SUFFIXES: &[&str] = &[
        "-web",
        "-api",
        "-app",
        "-frontend",
        "-backend",
        "-service",
        "-server",
        "-client",
        "-worker",
    ];
    for suffix in SUFFIXES {
        if let Some(stem) = name.strip_suffix(suffix) {
            if !stem.is_empty() {
                return stem;
            }
        }
    }
    name
}

fn find_railway_by_stem(stem: &str, results: &DiscoveryResults, claimed: &[bool]) -> Option<usize> {
    for (ri, rw) in results.railway.iter().enumerate() {
        if claimed[ri] {
            continue;
        }
        let project_name = rw
            .display_name
            .split(" / ")
            .next()
            .unwrap_or(&rw.display_name);
        if name_stem(project_name) == stem {
            return Some(ri);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gh_resource(full_name: &str, name: &str, org: &str) -> DiscoveredResource {
        DiscoveredResource {
            platform_id: full_name.into(),
            display_name: name.into(),
            group: org.into(),
            group_type: "organization".into(),
            archived: false,
            disabled: false,
        }
    }

    fn rw_resource(composite_id: &str, display: &str, workspace: &str) -> DiscoveredResource {
        DiscoveredResource {
            platform_id: composite_id.into(),
            display_name: display.into(),
            group: workspace.into(),
            group_type: "workspace".into(),
            archived: false,
            disabled: false,
        }
    }

    fn vc_resource(id: &str, name: &str, team: &str) -> DiscoveredResource {
        DiscoveredResource {
            platform_id: id.into(),
            display_name: name.into(),
            group: team.into(),
            group_type: "team".into(),
            archived: false,
            disabled: false,
        }
    }

    #[test]
    fn name_stem_strips_known_suffixes() {
        assert_eq!(name_stem("my-saas-web"), "my-saas");
        assert_eq!(name_stem("my-saas-api"), "my-saas");
        assert_eq!(name_stem("my-saas-app"), "my-saas");
        assert_eq!(name_stem("my-saas-frontend"), "my-saas");
        assert_eq!(name_stem("my-saas-backend"), "my-saas");
        assert_eq!(name_stem("my-saas-service"), "my-saas");
        assert_eq!(name_stem("my-saas-server"), "my-saas");
        assert_eq!(name_stem("my-saas-client"), "my-saas");
        assert_eq!(name_stem("my-saas-worker"), "my-saas");
    }

    #[test]
    fn name_stem_preserves_non_suffix() {
        assert_eq!(name_stem("my-saas"), "my-saas");
        assert_eq!(name_stem("api-core"), "api-core");
        assert_eq!(name_stem("dashboard"), "dashboard");
    }

    #[test]
    fn vercel_linked_repo_creates_linked_match() {
        let results = DiscoveryResults {
            github: vec![gh_resource("myorg/my-saas", "my-saas", "myorg")],
            railway: vec![],
            vercel: vec![(
                vc_resource("prj-001", "my-saas-web", "Lambda"),
                Some("myorg/my-saas".into()),
            )],
        };

        let candidates = build_correlations(&results);
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].name, "my-saas");
        assert_eq!(candidates[0].confidence, MatchConfidence::LinkedRepo);
        assert!(candidates[0].github.is_some());
        assert!(candidates[0].vercel.is_some());
    }

    #[test]
    fn stem_matching_across_platforms() {
        let results = DiscoveryResults {
            github: vec![gh_resource("myorg/my-saas", "my-saas", "myorg")],
            railway: vec![rw_resource(
                "proj:svc:env",
                "my-saas-api / api / production",
                "lambda-prod",
            )],
            vercel: vec![(vc_resource("prj-001", "my-saas-web", "Lambda"), None)],
        };

        let candidates = build_correlations(&results);
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].name, "my-saas");
        assert_eq!(candidates[0].confidence, MatchConfidence::ExactStem);
        assert!(candidates[0].github.is_some());
        assert!(candidates[0].railway.is_some());
        assert!(candidates[0].vercel.is_some());
    }

    #[test]
    fn unmatched_resources_become_standalone() {
        let results = DiscoveryResults {
            github: vec![
                gh_resource("myorg/my-saas", "my-saas", "myorg"),
                gh_resource("myorg/api-core", "api-core", "myorg"),
            ],
            railway: vec![],
            vercel: vec![(vc_resource("prj-001", "my-saas-web", "Lambda"), None)],
        };

        let candidates = build_correlations(&results);
        assert_eq!(candidates.len(), 2);

        let my_saas = candidates.iter().find(|c| c.name == "my-saas").unwrap();
        assert_eq!(my_saas.confidence, MatchConfidence::ExactStem);
        assert!(my_saas.github.is_some());
        assert!(my_saas.vercel.is_some());

        let api_core = candidates.iter().find(|c| c.name == "api-core").unwrap();
        assert_eq!(api_core.confidence, MatchConfidence::Unmatched);
        assert!(api_core.github.is_some());
        assert!(api_core.railway.is_none());
        assert!(api_core.vercel.is_none());
    }

    #[test]
    fn linked_repo_also_picks_up_railway_by_stem() {
        let results = DiscoveryResults {
            github: vec![gh_resource("myorg/my-saas", "my-saas", "myorg")],
            railway: vec![rw_resource(
                "proj:svc:env",
                "my-saas-api / api / production",
                "lambda-prod",
            )],
            vercel: vec![(
                vc_resource("prj-001", "my-saas-web", "Lambda"),
                Some("myorg/my-saas".into()),
            )],
        };

        let candidates = build_correlations(&results);
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].name, "my-saas");
        assert_eq!(candidates[0].confidence, MatchConfidence::LinkedRepo);
        assert!(candidates[0].github.is_some());
        assert!(candidates[0].railway.is_some());
        assert!(candidates[0].vercel.is_some());
    }

    #[test]
    fn candidate_to_config_conversion() {
        let candidate = CorrelationCandidate {
            name: "my-saas".into(),
            github: Some(gh_resource("myorg/my-saas", "my-saas", "myorg")),
            railway: Some(rw_resource(
                "proj:svc:env",
                "my-saas-api / api / production",
                "lambda-prod",
            )),
            vercel: Some(vc_resource("prj-001", "my-saas-web", "Lambda")),
            vercel_linked_repo: Some("myorg/my-saas".into()),
            confidence: MatchConfidence::LinkedRepo,
        };

        let config = candidate_to_config(&candidate);
        assert_eq!(config.name, "my-saas");
        assert_eq!(config.github_repo, Some("myorg/my-saas".into()));
        assert_eq!(config.railway_project, Some("proj:svc:env".into()));
        assert_eq!(config.railway_workspace, Some("lambda-prod".into()));
        assert_eq!(config.vercel_project, Some("prj-001".into()));
        assert_eq!(config.vercel_team, Some("Lambda".into()));
        assert!(config.branch_mapping.is_empty());
    }

    #[test]
    fn empty_discovery_produces_no_candidates() {
        let results = DiscoveryResults {
            github: vec![],
            railway: vec![],
            vercel: vec![],
        };
        let candidates = build_correlations(&results);
        assert!(candidates.is_empty());
    }
}
