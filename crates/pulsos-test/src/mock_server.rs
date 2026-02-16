use wiremock::matchers::{header, method, path, path_regex};
use wiremock::{Mock, MockServer, ResponseTemplate};

use crate::fixtures;

/// A mock GitHub API server with pre-configured responses.
pub struct MockGitHub {
    pub server: MockServer,
}

impl MockGitHub {
    pub async fn start() -> Self {
        let server = MockServer::start().await;

        // GET /repos/{owner}/{repo}/actions/runs
        Mock::given(method("GET"))
            .and(path_regex(r"^/repos/.+/.+/actions/runs"))
            .and(header("Authorization", "Bearer test-github-token"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(fixtures::github::workflow_runs_success())
                    .insert_header("x-ratelimit-limit", "5000")
                    .insert_header("x-ratelimit-remaining", "4999")
                    .insert_header("x-ratelimit-used", "1")
                    .insert_header("x-ratelimit-reset", "1739620000"),
            )
            .mount(&server)
            .await;

        // GET /user
        Mock::given(method("GET"))
            .and(path("/user"))
            .and(header("Authorization", "Bearer test-github-token"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(fixtures::github::user_response())
                    .insert_header("x-oauth-scopes", "repo, read:org")
                    .insert_header("x-ratelimit-limit", "5000")
                    .insert_header("x-ratelimit-remaining", "4999")
                    .insert_header("x-ratelimit-used", "1")
                    .insert_header("x-ratelimit-reset", "1739620000"),
            )
            .mount(&server)
            .await;

        // GET /user/repos
        Mock::given(method("GET"))
            .and(path("/user/repos"))
            .and(header("Authorization", "Bearer test-github-token"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(fixtures::github::repos_response()),
            )
            .mount(&server)
            .await;

        Self { server }
    }

    pub fn url(&self) -> String {
        self.server.uri()
    }
}

/// A mock Railway GraphQL server with pre-configured responses.
pub struct MockRailway {
    pub server: MockServer,
}

impl MockRailway {
    pub async fn start() -> Self {
        let server = MockServer::start().await;

        // POST /graphql/v2 — deployments query
        Mock::given(method("POST"))
            .and(path("/graphql/v2"))
            .and(wiremock::matchers::body_string_contains("deployments"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(fixtures::railway::deployments_response()),
            )
            .named("railway-deployments")
            .mount(&server)
            .await;

        // POST /graphql/v2 — me query
        Mock::given(method("POST"))
            .and(path("/graphql/v2"))
            .and(wiremock::matchers::body_string_contains("me"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(fixtures::railway::me_response()),
            )
            .named("railway-me")
            .mount(&server)
            .await;

        Self { server }
    }

    pub fn url(&self) -> String {
        self.server.uri()
    }
}

/// A mock Vercel API server with pre-configured responses.
pub struct MockVercel {
    pub server: MockServer,
}

impl MockVercel {
    pub async fn start() -> Self {
        let server = MockServer::start().await;

        // GET /v6/deployments
        Mock::given(method("GET"))
            .and(path_regex(r"^/v6/deployments"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(fixtures::vercel::deployments_response()),
            )
            .mount(&server)
            .await;

        // GET /v2/user
        Mock::given(method("GET"))
            .and(path("/v2/user"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(fixtures::vercel::user_response()),
            )
            .mount(&server)
            .await;

        // GET /v9/projects
        Mock::given(method("GET"))
            .and(path_regex(r"^/v9/projects"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(fixtures::vercel::projects_response()),
            )
            .mount(&server)
            .await;

        // GET /v2/teams
        Mock::given(method("GET"))
            .and(path_regex(r"^/v2/teams"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(fixtures::vercel::teams_response()),
            )
            .mount(&server)
            .await;

        Self { server }
    }

    pub fn url(&self) -> String {
        self.server.uri()
    }
}
