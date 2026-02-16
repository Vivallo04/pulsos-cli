//! Pre-built JSON fixture strings for each platform.

pub mod github {
    pub fn workflow_runs_success() -> serde_json::Value {
        serde_json::json!({
            "total_count": 2,
            "workflow_runs": [
                {
                    "id": 100001,
                    "name": "CI",
                    "head_branch": "main",
                    "head_sha": "abc123def456789",
                    "run_number": 42,
                    "event": "push",
                    "display_title": "Fix login bug",
                    "status": "completed",
                    "conclusion": "success",
                    "workflow_id": 99,
                    "html_url": "https://github.com/org/repo/actions/runs/100001",
                    "created_at": "2026-02-15T10:00:00Z",
                    "updated_at": "2026-02-15T10:05:00Z",
                    "run_started_at": "2026-02-15T10:00:30Z",
                    "actor": { "login": "vivallo", "id": 1, "avatar_url": null }
                },
                {
                    "id": 100002,
                    "name": "Deploy",
                    "head_branch": "main",
                    "head_sha": "abc123def456789",
                    "run_number": 41,
                    "event": "push",
                    "display_title": "Deploy to staging",
                    "status": "in_progress",
                    "conclusion": null,
                    "workflow_id": 100,
                    "html_url": "https://github.com/org/repo/actions/runs/100002",
                    "created_at": "2026-02-15T09:55:00Z",
                    "updated_at": "2026-02-15T10:00:00Z",
                    "run_started_at": "2026-02-15T09:55:10Z",
                    "actor": { "login": "bot", "id": 2, "avatar_url": null }
                }
            ]
        })
    }

    pub fn workflow_runs_empty() -> serde_json::Value {
        serde_json::json!({
            "total_count": 0,
            "workflow_runs": []
        })
    }

    pub fn user_response() -> serde_json::Value {
        serde_json::json!({
            "login": "vivallo",
            "id": 12345,
            "name": "Test User"
        })
    }

    pub fn repos_response() -> serde_json::Value {
        serde_json::json!([
            {
                "id": 1,
                "full_name": "myorg/my-saas",
                "name": "my-saas",
                "private": false,
                "archived": false,
                "disabled": false,
                "default_branch": "main",
                "html_url": "https://github.com/myorg/my-saas",
                "owner": { "login": "myorg", "id": 100, "type": "Organization" }
            },
            {
                "id": 2,
                "full_name": "myorg/api-core",
                "name": "api-core",
                "private": true,
                "archived": false,
                "disabled": false,
                "default_branch": "main",
                "html_url": "https://github.com/myorg/api-core",
                "owner": { "login": "myorg", "id": 100, "type": "Organization" }
            }
        ])
    }
}

pub mod railway {
    pub fn deployments_response() -> serde_json::Value {
        serde_json::json!({
            "data": {
                "deployments": {
                    "edges": [
                        {
                            "node": {
                                "id": "rw-deploy-001",
                                "status": "SUCCESS",
                                "createdAt": "2026-02-15T10:02:00Z",
                                "staticUrl": "my-api.up.railway.app"
                            }
                        },
                        {
                            "node": {
                                "id": "rw-deploy-002",
                                "status": "BUILDING",
                                "createdAt": "2026-02-15T10:05:00Z",
                                "staticUrl": null
                            }
                        }
                    ]
                }
            }
        })
    }

    pub fn me_response() -> serde_json::Value {
        serde_json::json!({
            "data": {
                "me": {
                    "id": "user-123",
                    "email": "test@lambda.co",
                    "name": "Test User"
                }
            }
        })
    }

    pub fn teams_response() -> serde_json::Value {
        serde_json::json!({
            "data": {
                "teams": {
                    "edges": [
                        {
                            "node": {
                                "id": "team-001",
                                "name": "lambda-prod"
                            }
                        }
                    ]
                }
            }
        })
    }

    pub fn projects_response() -> serde_json::Value {
        serde_json::json!({
            "data": {
                "projects": {
                    "edges": [
                        {
                            "node": {
                                "id": "proj-001",
                                "name": "my-saas-api",
                                "description": "API service",
                                "createdAt": "2025-01-15T10:00:00Z",
                                "services": {
                                    "edges": [
                                        { "node": { "id": "svc-001", "name": "api" } }
                                    ]
                                },
                                "environments": {
                                    "edges": [
                                        { "node": { "id": "env-001", "name": "production" } }
                                    ]
                                }
                            }
                        }
                    ]
                }
            }
        })
    }
}

pub mod vercel {
    pub fn deployments_response() -> serde_json::Value {
        serde_json::json!({
            "deployments": [
                {
                    "uid": "dpl_abc123",
                    "name": "my-saas-web",
                    "url": "my-saas-web-abc123.vercel.app",
                    "created": 1739613600000_u64,
                    "state": "READY",
                    "target": "production",
                    "creator": { "uid": "user-001", "username": "vivallo" },
                    "meta": {
                        "githubCommitSha": "abc123def456789",
                        "githubCommitRef": "main",
                        "githubCommitMessage": "Fix login bug",
                        "githubCommitAuthorName": "vivallo",
                        "githubCommitOrg": "myorg",
                        "githubCommitRepo": "my-saas"
                    }
                },
                {
                    "uid": "dpl_def456",
                    "name": "my-saas-web",
                    "url": "my-saas-web-def456.vercel.app",
                    "created": 1739610000000_u64,
                    "state": "BUILDING",
                    "target": null,
                    "creator": { "uid": "user-001", "username": "vivallo" }
                }
            ],
            "pagination": { "count": 2, "next": null, "prev": null }
        })
    }

    pub fn user_response() -> serde_json::Value {
        serde_json::json!({
            "user": {
                "uid": "user-001",
                "username": "vivallo",
                "email": "test@lambda.co",
                "name": "Test User"
            }
        })
    }

    pub fn projects_response() -> serde_json::Value {
        serde_json::json!({
            "projects": [
                {
                    "id": "prj-001",
                    "name": "my-saas-web",
                    "framework": "nextjs",
                    "link": {
                        "type": "github",
                        "repo": "myorg/my-saas",
                        "repoId": 12345,
                        "org": "myorg"
                    }
                }
            ],
            "pagination": { "count": 1, "next": null, "prev": null }
        })
    }

    pub fn teams_response() -> serde_json::Value {
        serde_json::json!({
            "teams": [
                {
                    "id": "team-001",
                    "name": "Lambda",
                    "slug": "lambda"
                }
            ],
            "pagination": { "count": 1, "next": null, "prev": null }
        })
    }
}
