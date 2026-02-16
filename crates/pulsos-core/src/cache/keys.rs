//! Cache keys use a hierarchical namespace:
//!   `{platform}:{resource_type}:{resource_id}:{sub_resource}`
//!
//! Examples:
//!   - `github:runs:myorg/my-saas`          — Last 5 workflow runs
//!   - `github:jobs:myorg/my-saas:12345`    — Jobs for run 12345
//!   - `railway:deployments:project-uuid`   — Last 5 deployments
//!   - `railway:instance:service-uuid:env-uuid` — Service instance
//!   - `vercel:deployments:project-id`      — Last 5 deployments
//!   - `vercel:projects:team-id`            — Project list for team
//!   - `meta:github:rate_limit`             — Current rate limit state
//!   - `meta:github:etag:myorg/my-saas`     — ETag for last request
//!   - `config:projects`                    — Serialized project list

pub fn github_runs_key(repo: &str) -> String {
    format!("github:runs:{repo}")
}

pub fn github_jobs_key(repo: &str, run_id: u64) -> String {
    format!("github:jobs:{repo}:{run_id}")
}

pub fn github_etag_key(repo: &str) -> String {
    format!("meta:github:etag:{repo}")
}

pub fn github_rate_limit_key() -> String {
    "meta:github:rate_limit".to_string()
}

pub fn railway_deployments_key(project_id: &str, service_id: &str, env_id: &str) -> String {
    format!("railway:deployments:{project_id}:{service_id}:{env_id}")
}

pub fn railway_instance_key(service_id: &str, env_id: &str) -> String {
    format!("railway:instance:{service_id}:{env_id}")
}

pub fn vercel_deployments_key(project_id: &str) -> String {
    format!("vercel:deployments:{project_id}")
}

pub fn vercel_projects_key(team_id: &str) -> String {
    format!("vercel:projects:{team_id}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_format() {
        assert_eq!(
            github_runs_key("myorg/my-saas"),
            "github:runs:myorg/my-saas"
        );
        assert_eq!(
            github_etag_key("myorg/my-saas"),
            "meta:github:etag:myorg/my-saas"
        );
        assert_eq!(
            railway_deployments_key("proj-123", "svc-1", "env-2"),
            "railway:deployments:proj-123:svc-1:env-2"
        );
        assert_eq!(
            railway_instance_key("svc-1", "env-2"),
            "railway:instance:svc-1:env-2"
        );
        assert_eq!(
            vercel_deployments_key("prj_abc"),
            "vercel:deployments:prj_abc"
        );
    }
}
