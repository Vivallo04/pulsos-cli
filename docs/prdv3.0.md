Version: 3.0.0 Date: February 15, 2026 Status: Draft Author: Lambda Engineering License: MIT (Open Source) Repository: github.com/lambdahq/pulsos

1. Executive Summary

Pulsos is a single, zero-dependency Rust binary that provides unified deployment monitoring across GitHub Actions, Railway, and Vercel. It correlates deployment events across all three platforms using commit SHA tracking, giving developers and DevOps teams a single pane of glass to understand their deployment pipeline health.

Pulsos calls each platform's API directly — it does not wrap or depend on any external CLI tool. A user installs one binary (via Homebrew, pre-built download, or cargo install) and has everything they need. No Node.js, no gh, no railway, no vercel CLI required.

Built with Rust for performance and reliability, Pulsos is designed to be open-sourced under the MIT license. It will be dogfooded internally at Lambda before public release.

1.1 Core Value Proposition

One commit, three platforms, one view. Pulsos correlates a GitHub commit SHA to its Railway deployment and Vercel preview, showing the complete deployment lifecycle in a single terminal command.

1.2 Why Zero Dependencies?

The original PRD required users to install gh, railway (which requires Node.js 16+), and vercel (also Node.js) before Pulsos could function. This is a dealbreaker for adoption and undermines Lambda's positioning as a systems-quality engineering organization. A Rust binary that requires Node.js to function is architecturally incoherent.

Every feature Pulsos needs is available through direct API calls: GitHub's REST API, Railway's GraphQL API, and Vercel's REST API. The platform CLIs add nothing for read-only monitoring. Authentication is handled natively through token-based flows.

2. Design Philosophy

Pulsos follows a set of core principles that guide every architectural and UX decision:

Zero external dependencies: Pulsos is a single binary. No gh, railway, vercel, Node.js, or any other tool required. All data is fetched via direct API calls.

Read-only by design: Pulsos never needs write access to any platform and only performs read operations. Some platforms (notably GitHub classic PATs for private Actions data) still require write-capable token scopes due to API design. Pulsos cannot modify your repositories, trigger deployments, or change any configuration.

API-first, CLI-optional: All data fetching goes through platform APIs directly (api.github.com, backboard.railway.com/graphql/v2, api.vercel.com). If a user happens to have platform CLIs installed, Pulsos can detect and reuse their existing authentication tokens as a convenience — but never requires them.

Two-step time-to-value: A new user should see their deployment dashboard within two interactions: install the binary, run pulsos. The first-run wizard handles authentication, discovery, and initial view in a single pass.

Graceful degradation: If a platform API is unavailable, rate-limited, or the user is offline, Pulsos continues to function using cached data with clear staleness indicators. It never crashes or hangs due to network issues.

Honest about limitations: Where platform APIs don't support a feature cleanly (such as Railway's lack of continuous health monitoring or commit SHA correlation), Pulsos documents the limitation rather than faking accuracy.

Configuration as code: All views, repo selections, and preferences are stored in human-readable TOML files that can be version-controlled and shared across teams.

3. Architecture

3.1 High-Level Architecture

┌──────────────────────────────────────────────────────────────────┐
│                        Pulsos Binary (Rust)                       │
├──────────────────────────────────────────────────────────────────┤
│                                                                   │
│  ┌─────────────────────────────────────────────────────────────┐ │
│  │                     Command Layer (clap)                     │ │
│  │  pulsos (first-run) │ status │ auth │ repos │ views │ doctor│ │
│  └────────────────────────────┬────────────────────────────────┘ │
│                               │                                   │
│  ┌────────────────────────────▼────────────────────────────────┐ │
│  │                   Platform Adapter Layer                     │ │
│  │                                                              │ │
│  │  ┌──────────────┐  ┌───────────────┐  ┌──────────────────┐  │ │
│  │  │   GitHub      │  │   Railway      │  │    Vercel        │  │ │
│  │  │   Adapter     │  │   Adapter      │  │    Adapter       │  │ │
│  │  │              │  │               │  │                │  │ │
│  │  │  REST API    │  │  GraphQL API  │  │  REST API      │  │ │
│  │  │  api.github  │  │  backboard.   │  │  api.vercel    │  │ │
│  │  │  .com        │  │  railway.com  │  │  .com          │  │ │
│  │  └──────────────┘  └───────────────┘  └──────────────────┘  │ │
│  └─────────────────────────────────────────────────────────────┘ │
│                               │                                   │
│  ┌────────────────────────────▼────────────────────────────────┐ │
│  │                   Core Services Layer                        │ │
│  │                                                              │ │
│  │  ┌──────────────┐  ┌───────────────┐  ┌──────────────────┐  │ │
│  │  │ Correlation  │  │  Cache        │  │  Credential      │  │ │
│  │  │ Engine       │  │  Manager      │  │  Store           │  │ │
│  │  │              │  │               │  │                  │  │ │
│  │  │ SHA matching │  │ sled DB +     │  │ OS keyring +     │  │ │
│  │  │ + heuristics │  │ TTL + stale   │  │ token detection  │  │ │
│  │  └──────────────┘  └───────────────┘  └──────────────────┘  │ │
│  └─────────────────────────────────────────────────────────────┘ │
│                               │                                   │
│  ┌────────────────────────────▼────────────────────────────────┐ │
│  │                    Presentation Layer                        │ │
│  │                                                              │ │
│  │  ┌──────────────┐  ┌───────────────┐  ┌──────────────────┐  │ │
│  │  │ Table Output │  │  TUI (ratatui)│  │  JSON Output     │  │ │
│  │  │ (default)    │  │  (--watch)    │  │  (--format json) │  │ │
│  │  └──────────────┘  └───────────────┘  └──────────────────┘  │ │
│  └─────────────────────────────────────────────────────────────┘ │
│                                                                   │
└──────────────────────────────────────────────────────────────────┘


3.2 Platform Adapters — What Each API Actually Provides

3.2.1 GitHub Adapter (REST API)

Endpoint: https://api.github.com Auth: Bearer token (Personal Access Token or OAuth token) Protocol: REST (JSON)

API calls used by Pulsos:

Endpoint

Purpose

Fields Used

GET /user/orgs

List user's organizations

login, id

GET /user/repos

List user's personal repos

full_name, private, archived

GET /orgs/{org}/repos

List repos in an organization

full_name, private, archived, permissions

GET /repos/{owner}/{repo}/actions/runs

List workflow runs

id, name, head_sha, head_branch, status, conclusion, created_at, updated_at, run_started_at, workflow_id

GET /repos/{owner}/{repo}/actions/runs/{id}/jobs

List jobs in a run

name, status, conclusion, started_at, completed_at, steps[]

GET /repos/{owner}/{repo}/actions/workflows

List workflows

id, name, path, state

GET /repos/{owner}/{repo}/collaborators/{user}/permission

Check user permissions

permission (admin/write/read/none)

Rate limits: 5,000 requests/hour for authenticated users. Tracked via X-RateLimit-Remaining response header.

What works well: Complete commit SHA tracking. Every workflow run exposes head_sha which enables exact correlation with Vercel deployments. Job-level detail (steps, durations) is fully available.

What doesn't work: Organization/enterprise ruleset workflows don't display workflow names (GitHub API limitation).

3.2.2 Railway Adapter (GraphQL API)

Endpoint: https://backboard.railway.com/graphql/v2 Auth: Bearer token (Account Token, Workspace Token, or Project Token) Protocol: GraphQL

Key queries used by Pulsos:

# List all projects in a workspace
query {
  projects(teamId: "workspace-id") {
    edges {
      node {
        id
        name
        description
        services {
          edges {
            node {
              id
              name
            }
          }
        }
        environments {
          edges {
            node {
              id
              name
            }
          }
        }
      }
    }
  }
}

# Get service instance with latest deployment
query {
  serviceInstance(serviceId: "xxx", environmentId: "yyy") {
    id
    serviceName
    healthcheckPath
    region
    numReplicas
    restartPolicyType
    latestDeployment {
      id
      status
      createdAt
    }
  }
}

# List deployments for a service
query {
  deployments(first: 10, input: {
    projectId: "xxx"
    environmentId: "yyy"
    serviceId: "zzz"
  }) {
    edges {
      node {
        id
        status
        createdAt
      }
    }
  }
}


Deployment statuses: BUILDING, CRASHED, DEPLOYING, FAILED, INITIALIZING, NEEDS_APPROVAL, QUEUED, REMOVED, REMOVING, SKIPPED, SLEEPING, SUCCESS, WAITING

What works well: Full project/service/environment hierarchy is queryable. Deployment status is reliable. GraphQL means we request exactly the fields we need, minimizing bandwidth.

Critical limitations Pulsos must be honest about:

No continuous health monitoring. Railway's healthcheck endpoint is only used during deployment — it checks that the new deployment serves a 200 before routing traffic. After that, Railway does not poll the healthcheck. Pulsos can show "latest deployment status" (SUCCESS, CRASHED, etc.) but cannot show "service is currently healthy." The PRD must not call this "health" — it's "deployment status."

No commit SHA in deployment data. Railway's deployment object does not reliably expose the git commit SHA that triggered the deployment. The serviceInstance query returns deployment id, status, and createdAt, but not the commit. Correlation with GitHub must use heuristic matching (timestamp + branch + environment), which is inherently unreliable during high-frequency deployment scenarios.

Project-scoped access. Railway's CLI requires railway link to a specific project before showing data. Pulsos bypasses this by using the GraphQL API directly with account-level tokens, which can query across all projects.

3.2.3 Vercel Adapter (REST API)

Endpoint: https://api.vercel.com Auth: Bearer token (Access Token) Protocol: REST (JSON)

API calls used by Pulsos:

Endpoint

Purpose

Fields Used

GET /v2/teams

List teams

id, name, slug

GET /v9/projects

List projects (optionally per team)

id, name, framework, link.type, link.repo

GET /v6/deployments

List deployments

uid, name, url, state, created, meta.githubCommitSha, meta.githubCommitRef, meta.githubCommitMessage, target, creator

GET /v13/deployments/{id}

Deployment detail

Full deployment object including build logs reference

Deployment states: QUEUED, BUILDING, READY, ERROR, CANCELED

What works well: Vercel exposes meta.githubCommitSha on every git-triggered deployment. This means GitHub-to-Vercel correlation is exact — match the SHA directly. You can even filter deployments by commit SHA using the query parameter meta-githubCommitSha={sha} (undocumented but functional).

Vercel also exposes link.repo on each project, which tells you exactly which GitHub repository is connected. This gives Pulsos automatic correlation mapping without user configuration.

What doesn't work: Rate limits vary by Vercel plan and aren't always clearly documented. Hobby plans have stricter limits. Pulsos detects the plan tier from initial auth and adjusts polling accordingly.

3.3 Correlation Engine

The correlation engine is the core differentiator of Pulsos. It maps a single commit across all three platforms.

3.3.1 Correlation Methods by Platform Pair

GitHub → Vercel (exact match): Both platforms expose the git commit SHA. Vercel deployments include meta.githubCommitSha which matches GitHub's head_sha on workflow runs. Additionally, Vercel projects expose link.repo which maps directly to a GitHub repository. This correlation is automatic and requires zero user configuration.

GitHub → Railway (heuristic match): Railway does not reliably expose commit SHAs. Correlation uses a multi-signal heuristic:

Explicit mapping (highest confidence): User defines the relationship in config.toml — "Railway project X corresponds to GitHub repo Y"

Timestamp proximity (medium confidence): If a Railway deployment starts within 120 seconds of a GitHub workflow run completing on the same branch, they're likely related

Connected repo detection (medium confidence): If the Railway service source references a GitHub repo URL, use that mapping

Pulsos displays a confidence indicator on every correlated event:

● Exact (SHA match — GitHub↔Vercel only)

◐ High (explicit mapping + timestamp match)

○ Low (timestamp-only heuristic)

? Unmatched (no correlation found)

3.3.2 Explicit Mapping Configuration

Users can define exact platform relationships to improve correlation accuracy:

# ~/.config/pulsos/config.toml

[[correlations]]
github_repo = "myorg/api-core"
railway_project = "api-core-prod"
railway_environment = "production"
vercel_project = "api-core-docs"
branch_mapping = { main = "production", develop = "staging" }


This mapping also enables Pulsos to show unified project views — "here's everything about api-core across all platforms."

3.3.3 Health Scoring

Each project gets a computed health score (0–100) based on weighted platform status:

GitHub CI success rate over last 10 runs: 40% weight

Railway latest deployment status: 35% weight

Vercel latest deployment status: 25% weight

For Railway, the score is based on deployment status, not service health (because Railway does not provide continuous health monitoring). A SUCCESS deployment gets full marks; CRASHED or FAILED gets zero; BUILDING/DEPLOYING gets neutral marks.

3.4 Credential Store

Tokens are stored using OS-native secure storage with the following backend chain:

OS

Primary Backend

Fallback

macOS

Keychain Access (via security CLI or Security framework)

Encrypted file at ~/.config/pulsos/credentials.enc

Linux

Secret Service D-Bus API (GNOME Keyring, KDE Wallet)

Encrypted file

Windows

Credential Manager (via wincred API)

Encrypted file

The encrypted file fallback uses AES-256-GCM with a key derived from the host identifier (Linux: `/etc/machine-id`, macOS: IOPlatformUUID, Windows: MachineGuid) plus a per-install random salt. This fallback protects against casual plaintext token disclosure but is weaker than OS keyrings because machine identifiers are not user secrets.

Tokens in memory are wrapped in the secrecy crate's Secret<String> type, which zeroes memory on drop and prevents accidental logging.

3.4.1 Token Detection from Existing CLIs

If the user already has platform CLIs installed and authenticated, Pulsos can detect and reuse their tokens. This is a convenience feature, not a requirement.

CLI

Token Location

How Pulsos Reads It

gh

~/.config/gh/hosts.yml → oauth_token field

YAML parse, extract token for github.com host

railway

~/.railway/config.json → token field

JSON parse

vercel

~/.vercel/auth.json → token field

JSON parse

Environment vars

GITHUB_TOKEN, GH_TOKEN, RAILWAY_TOKEN, RAILWAY_API_TOKEN, VERCEL_TOKEN

Direct read

During pulsos auth, if Pulsos detects an existing token, it offers to reuse it: "Found existing GitHub token from gh CLI. Use this token? [Y/n]". The user can always choose to enter a new token instead.

3.5 Cache Architecture

Pulsos maintains a local cache at ~/.cache/pulsos/ using sled (an embedded key-value database). Each cache entry stores the serialized API response, the timestamp it was fetched, and a TTL.

Data Type

Default TTL

Max Staleness

Behavior When Stale

GitHub workflow runs

30 seconds

1 hour

Show with yellow age indicator

Railway deployment status

30 seconds

1 hour

Show with yellow age indicator

Vercel deployments

30 seconds

1 hour

Show with yellow age indicator

Auth token validity

5 minutes

15 minutes

Prompt re-authentication

Repository/project list

1 hour

24 hours

Show cached, suggest repos sync

Organization/workspace metadata

6 hours

7 days

Show cached list

Correlation mappings

1 hour

24 hours

Use last known mapping

The cache serves two purposes: reducing API calls during normal operation (critical for GitHub's 5,000/hour limit) and providing fallback data during outages or offline usage.

3.6 Error Recovery & Offline Behavior

3.6.1 Rate Limit Handling

GitHub (5,000 requests/hour):

Track remaining quota via X-RateLimit-Remaining response header on every request

When below 10% remaining, automatically reduce polling frequency (double the interval)

When exhausted, display time until reset and serve cached data exclusively

Message: "GitHub API rate limit reached. Showing cached data (3 min old). Resets in 12 minutes."

Railway (GraphQL, limits less documented):

Use exponential backoff on 429 responses: 1s → 2s → 4s → 8s → 16s → 32s → 60s cap

After 3 consecutive 429s, switch to cache-only for Railway for 5 minutes

Log the incident for pulsos doctor diagnostics

Vercel (varies by plan):

Respect Retry-After header when present

Track per-endpoint limits separately (project list vs. deployment list vs. deployment detail)

Detect plan tier from initial responses and adjust base polling interval: Hobby plans poll every 15s instead of 5s

3.6.2 Network Failure Modes

Failure Mode

Detection

Response

DNS resolution failure

Connection error before TLS handshake

Serve cached data, show "Offline" per platform

Connection timeout

No response within 10 seconds

Retry once with 5s timeout, then fall back to cache

TLS handshake failure

Certificate or protocol error

Log error, serve cached data, surface in doctor

HTTP 5xx (server error)

Platform API returns 500–599

Retry with backoff (1s, 2s, 4s), then cache

HTTP 401/403 (auth error)

Token expired or revoked

Show "Auth expired" per platform, prompt pulsos auth

Partial/malformed response

JSON parse failure

Discard response, serve previous cached data

3.6.3 Offline Mode

When fully offline (all three platforms unreachable), Pulsos enters offline mode:

All views render using cached data exclusively

Each data point shows its age: "GitHub CI: passed (cached 47m ago)"

The status bar shows a persistent offline indicator

No retry loops or background polling (saves battery on laptops)

When connectivity is restored, Pulsos automatically detects it on the next poll cycle and refreshes all stale data

$ pulsos status

[OFFLINE] Showing cached data. Last sync: 47 minutes ago.

Project        GitHub CI        Railway          Vercel
──────────────────────────────────────────────────────────
my-saas        ✓ passed (47m)   SUCCESS (47m)    ✓ ready (47m)
api-core       ✓ passed (47m)   SUCCESS (47m)    —
auth-service   ✗ failed (47m)   SUCCESS (47m)    —

Network connectivity lost. Will refresh automatically when restored.


3.6.4 Staleness Indicators

Three-tier system for data freshness:

Tier

Condition

Display

Fresh

Data younger than TTL (30s)

No indicator — clean output

Stale

Between TTL and max staleness

Yellow age label: (3m ago)

Expired

Older than max staleness

Red warning: (STALE: 2h ago)

3.6.5 Error Reporting

Pulsos provides actionable guidance rather than raw error messages:

# Instead of:
Error: reqwest::Error { kind: Request, source: hyper::Error(Connect, ...) }

# Pulsos shows:
Could not reach api.github.com.

  Possible causes:
  1. No internet connection
  2. GitHub is experiencing an outage (check githubstatus.com)
  3. Firewall or proxy blocking the request

  Showing cached data from 12 minutes ago.
  Run `pulsos doctor` for full diagnostics.


4. Authentication System

4.1 Authentication Strategy

Pulsos uses token-based authentication for all three platforms. No browser automation, no OAuth complexity, no CLI dependencies. The auth flow is designed to work everywhere: local development, SSH sessions, CI/CD pipelines, and headless servers.

The primary path is: "Create a token on the platform's website, paste it into Pulsos." This is simple, universal, and secure.

4.2 Required API Scopes

Pulsos is read-only by design. These are the minimum scopes required per platform:

Platform

Scopes Required

Why

Access Level

GitHub

repo, read:org

Read workflow runs, list org repos

Read API usage (token itself is write-capable)

Railway

Account Token or Workspace Token

Query projects, services, deployments via GraphQL

Read-only

Vercel

Full Access Token (read scope)

List teams, projects, deployments

Read-only

Pulsos only performs read operations, but GitHub classic PATs require the write-capable `repo` scope to access private Actions data. Pulsos validates scopes and warns about over-privileged tokens (for example, `delete_repo`) while still functioning. This is a GitHub platform limitation; prefer fine-grained PATs with read-only repository permissions where possible.

4.3 GitHub Authentication

Method 1: Personal Access Token (recommended)

$ pulsos auth github

  Create a GitHub Personal Access Token:
  1. Visit https://github.com/settings/tokens/new
  2. Select scopes: repo, read:org (classic PAT; note `repo` is write-capable)
  3. Generate and copy the token

  Paste your token: ghp_****************************

  ✓ Authenticated as @vivallo (3 organizations found)
  Token stored in system keychain.


Method 2: Reuse existing gh CLI token

$ pulsos auth github

  Found existing token from gh CLI (~/.config/gh/hosts.yml).
  Use this token? [Y/n]: Y

  ✓ Authenticated as @vivallo (3 organizations found)


Method 3: Environment variable

export GITHUB_TOKEN=ghp_xxx
pulsos auth github --from-env

# Or for CI/CD, skip interactive prompts entirely:
pulsos auth --ci --github-token "$GITHUB_TOKEN"


Method 4: OAuth Device Flow (optional, for users who prefer browser auth)

Pulsos can implement GitHub's OAuth Device Flow directly without the gh CLI:

POST to https://github.com/login/device/code with Pulsos's OAuth App client ID

Display the device code to the user and open the browser to github.com/login/device

Poll https://github.com/login/oauth/access_token until the user authorizes

This requires registering Pulsos as a GitHub OAuth App (which Lambda would do for the open-source project). Deferred to v1.1 — token paste covers v1.0.

4.4 Railway Authentication

Method 1: Account/Workspace Token (recommended)

$ pulsos auth railway

  Create a Railway Account Token:
  1. Visit https://railway.app/account/tokens
  2. Create a new token (Account level for cross-project access)
  3. Copy the token

  Paste your token: ****************************

  ✓ Authenticated as vivallo@lambda.co
  Found 2 workspaces, 8 projects.
  Token stored in system keychain.


Method 2: Reuse existing Railway CLI token

$ pulsos auth railway

  Found existing token from Railway CLI (~/.railway/config.json).
  Use this token? [Y/n]: Y

  ✓ Authenticated as vivallo@lambda.co


Method 3: Environment variable

export RAILWAY_API_TOKEN=xxx   # Account/workspace token
pulsos auth railway --from-env


Token types and their access scope:

Token Type

Scope

Use Case

Account Token

All workspaces, all projects

Recommended for Pulsos (full visibility)

Workspace Token

Single workspace, all projects within it

Team-scoped monitoring

Project Token

Single project, single environment

Limited — not recommended for Pulsos

Pulsos warns if a Project Token is used, since it can only monitor one environment of one project.

4.5 Vercel Authentication

Method 1: Access Token (recommended)

$ pulsos auth vercel

  Create a Vercel Access Token:
  1. Visit https://vercel.com/account/tokens
  2. Set scope to your team (or "Full Account")
  3. Set expiration (recommended: 90 days)
  4. Copy the token

  Paste your token: ****************************

  ✓ Authenticated as lambda-team
  Found 1 team, 6 projects.
  Token stored in system keychain.

  Note: This token expires on 2026-05-16.
  Pulsos will warn you 7 days before expiration.


Method 2: Reuse existing Vercel CLI token

Reads from ~/.vercel/auth.json. Note: Vercel browser-session tokens have a 10-day inactivity timeout. Pulsos warns about this if detected.

Method 3: Environment variable

export VERCEL_TOKEN=xxx
pulsos auth vercel --from-env


4.6 Auth Status Dashboard

$ pulsos auth status

Platform   Status      User              Method        Expires
────────────────────────────────────────────────────────────────
GitHub     ✓ Valid     @vivallo          Token (PAT)   Never
Railway    ✓ Valid     v@lambda.co       Account Token Never
Vercel     ⚠ Expiring lambda-team       Access Token  2026-02-22 (7 days)
                                     → Run: pulsos auth vercel


5. First-Run Experience

The first-run experience is the most critical UX flow in Pulsos. It must get the user from zero to seeing their deployment dashboard in a single, guided session.

5.1 Golden Path (Two Interactions)

Interaction 1: Install

# macOS
brew install lambdahq/tap/pulsos

# Linux / macOS (pre-built binary)
curl -fsSL https://pulsos.dev/install.sh | sh

# From source (if user has Rust toolchain)
cargo install pulsos

# Windows
scoop install pulsos
# or download from GitHub Releases


Interaction 2: Run

$ pulsos

  Welcome to Pulsos! Let's set up your deployment monitoring.

  ─── Step 1/3: GitHub ───────────────────────────────────────

  Found existing token from gh CLI. Use it? [Y/n]: Y
  ✓ Authenticated as @vivallo

  Scanning organizations...
  Found 47 repositories across 3 organizations.

  Select repositories to monitor (Space to toggle, Enter to confirm):
    myorg (Owner)
      [✓] my-saas
      [✓] api-core
      [✓] auth-service
      [ ] legacy-monolith (archived)
      [ ] experiments (archived)
    client-work (Member)
      [✓] client-portal
    personal
      [ ] dotfiles
      [ ] blog

  ✓ 4 repositories selected

  ─── Step 2/3: Railway ──────────────────────────────────────

  No existing Railway token found.
  Create one at https://railway.app/account/tokens

  Paste your token (or press Enter to skip): ****

  ✓ Authenticated as vivallo@lambda.co
  Found 2 workspaces.

  Select projects to monitor:
    lambda-prod (Workspace)
      [✓] my-saas-api
      [✓] api-core-prod
      [✓] auth-service-prod
    lambda-staging (Workspace)
      [✓] my-saas-staging
      [ ] api-core-staging

  ✓ 4 projects selected

  ─── Step 3/3: Vercel ───────────────────────────────────────

  No existing Vercel token found.
  Create one at https://vercel.com/account/tokens

  Paste your token (or press Enter to skip): ****

  ✓ Authenticated as lambda-team
  Found 1 team.

  Select projects to monitor:
    lambda (Team)
      [✓] my-saas-web
      [✓] docs-portal
      [ ] marketing-site

  ✓ 2 projects selected

  ─── Auto-Correlating Projects ──────────────────────────────

  Detected connections:
    my-saas:
      GitHub: myorg/my-saas
      Railway: my-saas-api (lambda-prod)
      Vercel: my-saas-web (via linked GitHub repo)
    api-core:
      GitHub: myorg/api-core
      Railway: api-core-prod (lambda-prod)
      Vercel: — (not connected)

  ✓ Configuration saved to ~/.config/pulsos/config.toml

  ─── Your Dashboard ─────────────────────────────────────────

  Project        GitHub CI     Railway         Vercel
  ─────────────────────────────────────────────────────
  my-saas        ✓ passed      SUCCESS         ✓ ready
  api-core       ✓ passed      SUCCESS         —
  auth-service   ✗ failed      SUCCESS         —
  client-portal  ✓ passed      —               —
  docs-portal    —             —               ✓ ready

  Run `pulsos status --watch` for live monitoring.
  Run `pulsos` again to see this dashboard anytime.


5.2 Skipping Platforms

Any platform can be skipped during setup. Pulsos works with one, two, or all three platforms. If a user only uses GitHub and Vercel (no Railway), the Railway column simply shows "—" and correlation works between the two connected platforms.

5.3 Subsequent Runs

After first-run setup, pulsos (no arguments) shows the status dashboard directly. The first-run wizard only triggers when no config file exists at ~/.config/pulsos/config.toml.

5.4 Auto-Correlation Detection

During first-run, Pulsos attempts to automatically detect project relationships:

Vercel → GitHub: Vercel projects expose link.repo (e.g., myorg/my-saas). If this matches a monitored GitHub repo, correlation is automatic.

Railway → GitHub: If a Railway service source references a GitHub repository URL, Pulsos extracts the repo name and matches it. Less reliable than Vercel but works for git-connected Railway services.

Manual mapping: For any unmatched projects, the wizard asks: "Railway project 'api-core-prod' — which GitHub repo does this correspond to?" and lets the user select from the list.

6. CLI Interface

6.1 Top-Level Commands

pulsos [GLOBAL_OPTS] <COMMAND>

Commands:
  (none)      First-run wizard (if unconfigured) or status dashboard
  status      Unified status view
  auth        Authentication management
  repos       Repository & project discovery
  views       View management (create, list, delete)
  doctor      Diagnostics & troubleshooting
  config      Configuration management
  help        Print help for any command

Global Options:
  --format <table|json|compact>   Output format (default: table)
  --no-color                      Disable color output
  --verbose                       Show debug information
  --config <path>                 Custom config file path


6.2 Status Command (Primary Interface)

pulsos status [OPTIONS] [PROJECT_NAME]

Options:
  --platform <github|railway|vercel>   Filter to one platform
  --view <view_name>                   Use a saved view
  --watch                              Live-updating TUI mode
  --branch <pattern>                   Filter by branch
  --format <table|json|compact>        Output format

Examples:
  pulsos status                        # Everything, all platforms
  pulsos status my-saas                # One project, all platforms
  pulsos status --platform github      # All repos, GitHub only
  pulsos status --watch                # Live TUI mode (ratatui)
  pulsos status --view production      # Use saved view


6.3 Authentication Commands

pulsos auth [SUBCOMMAND]

Subcommands:
  (none)      Interactive auth for all platforms
  status      Check auth status across platforms
  github      Authenticate with GitHub
  railway     Authenticate with Railway
  vercel      Authenticate with Vercel
  logout      Logout from one or all platforms
  refresh     Refresh expiring tokens

CI/CD Mode:
  pulsos auth --ci \
    --github-token "$GITHUB_TOKEN" \
    --railway-token "$RAILWAY_TOKEN" \
    --vercel-token "$VERCEL_TOKEN"


6.4 Repository & Project Commands

pulsos repos [SUBCOMMAND]

Subcommands:
  sync        Discover + select + save (all platforms, single step)
  list        Show currently tracked repos/projects across all platforms
  add         Add a specific repo/project by name
  remove      Remove a repo/project from tracking
  groups      Manage logical groupings
  verify      Check permissions on tracked resources
  correlate   Manually set or edit platform correlations

Examples:
  pulsos repos sync                    # Full discovery + selection wizard
  pulsos repos sync --auto             # Auto-include via config patterns
  pulsos repos list                    # Show tracked resources
  pulsos repos add github:myorg/new-service
  pulsos repos add railway:new-api --workspace lambda-prod
  pulsos repos correlate my-saas       # Edit correlation for a project
  pulsos repos groups create backend github:myorg/api github:myorg/worker


6.5 View Commands

pulsos views [SUBCOMMAND]

Subcommands:
  list        List all configured views
  show        Display view configuration details
  create      Interactive creation wizard
  edit        Edit an existing view
  delete      Delete a view
  templates   List available view templates
  validate    Validate all resources in a view exist
  export      Export view as JSON
  import      Import view from JSON


6.6 Doctor Command

The doctor command diagnoses common issues. It runs automatically when pulsos status fails and can be invoked manually. This single command prevents the majority of support requests.

$ pulsos doctor

Pulsos Doctor v3.0.0
════════════════════════════════════════════

System
  OS:              macOS 15.3 (arm64)                    ✓
  Shell:           zsh 5.9                                ✓
  Terminal:        iTerm2 3.5                              ✓

Authentication
  GitHub:          @vivallo (PAT)                         ✓
  Railway:         v@lambda.co (Account Token)            ✓
  Vercel:          lambda-team (Access Token)             ⚠ expires in 7 days

API Connectivity
  api.github.com:          reachable (45ms)               ✓
  backboard.railway.com:   reachable (62ms)               ✓
  api.vercel.com:          reachable (38ms)               ✓

Rate Limits
  GitHub:          4,847 / 5,000 remaining (resets 14:32) ✓
  Railway:         OK                                     ✓
  Vercel:          OK                                     ✓

Tracked Resources
  GitHub repos:    4 repos across 2 orgs                  ✓
  Railway:         4 projects across 2 workspaces         ✓
  Vercel:          2 projects in 1 team                   ✓
  Permissions:     All readable                           ✓

Correlations
  Exact (SHA):     2 project pairs (GitHub↔Vercel)        ✓
  Mapped:          2 project pairs (GitHub↔Railway)       ✓
  Unmatched:       1 project (client-portal)              ⚠

Cache
  Location:        ~/.cache/pulsos/                       ✓
  Size:            2.3 MB (47 entries)                    ✓
  Oldest entry:    2 hours ago                            ✓

Optional CLI Detection
  gh CLI:          2.45.0 (token detected and reusable)   ✓
  railway CLI:     not installed (not required)            ─
  vercel CLI:      not installed (not required)            ─

Result: 2 warnings, 0 errors
  → Run `pulsos auth vercel` to refresh expiring token.
  → Run `pulsos repos correlate client-portal` to link platforms.


7. Platform Organization Models

Each platform has a different organizational hierarchy. Pulsos maps them correctly rather than pretending they're identical.

7.1 GitHub: Users → Organizations → Repositories → Workflows

User (@vivallo)
├── Personal Repositories
│   ├── dotfiles
│   └── blog
├── Organization: myorg (Owner)
│   ├── my-saas          → Workflows: ci.yml, deploy.yml
│   ├── api-core         → Workflows: ci.yml
│   └── auth-service     → Workflows: ci.yml, security.yml
└── Organization: client-work (Member)
    └── client-portal    → Workflows: ci.yml


Discovery: GET /user/repos + GET /user/orgs → GET /orgs/{org}/repos Permission check: GET /repos/{owner}/{repo}/collaborators/{user}/permission

7.2 Railway: Users → Workspaces → Projects → Services → Environments

User (vivallo@lambda.co)
├── Workspace: lambda-prod
│   ├── Project: my-saas-api
│   │   ├── Service: api        → Environments: production, staging
│   │   └── Service: worker     → Environments: production, staging
│   └── Project: api-core-prod
│       └── Service: api        → Environments: production
└── Workspace: lambda-staging
    └── Project: my-saas-staging
        └── Service: api        → Environments: staging


Railway does not use the term "organization." The equivalent concept is "Workspace." A user can belong to multiple workspaces, each containing multiple projects. Each project contains services, and each service can exist in multiple environments.

Discovery: GraphQL query for projects scoped to workspace → services per project → environments per project Key difference from GitHub: There is no concept of a "workflow" or "CI run." Railway's deployments are triggered by git pushes or manual deploys — the deployment itself is the atomic unit.

7.3 Vercel: Users → Teams → Projects → Deployments

User (vivallo)
├── Personal Projects
│   └── blog-site
└── Team: lambda
    ├── Project: my-saas-web    → Linked repo: myorg/my-saas
    │   ├── Production deployment
    │   └── Preview deployments (per PR)
    └── Project: docs-portal    → Linked repo: myorg/docs
        ├── Production deployment
        └── Preview deployments


Vercel uses "Team" as its organizational unit. Projects belong to either the user's personal scope or a team. Each project is linked to a Git repository and has one production deployment plus multiple preview deployments.

Discovery: GET /v2/teams → GET /v9/projects?teamId={id} → GET /v6/deployments?projectId={id} Key advantage: Projects expose link.repo which gives automatic correlation with GitHub.

7.4 Unified Project Model

Pulsos maps all three platforms into a unified "Project" concept:

# Internal representation (not directly user-facing)

[[projects]]
name = "my-saas"  # Unified name, used in CLI and TUI

[projects.github]
repo = "myorg/my-saas"
workflows = ["ci.yml", "deploy.yml"]

[projects.railway]
project = "my-saas-api"
workspace = "lambda-prod"
services = ["api", "worker"]
environment = "production"

[projects.vercel]
project = "my-saas-web"
team = "lambda"
include_previews = true


8. View System

8.1 Interactive View Creation

The pulsos views create command walks users through a TUI wizard:

View name and description

Project selection: Multi-select from tracked projects (fuzzy filter)

Platform filtering: Which platforms to show (all, or specific ones)

Branch filtering: main only, main + release/*, all branches, or custom pattern

Status filtering: Which statuses to show (passed, failed, running, all)

Save location: Global config or project-specific .pulsos.toml

8.2 View Templates

Template

GitHub

Railway

Vercel

Use Case

full-stack-production

CI + deploy workflows, main branch

Production services

Production deployments

End-to-end production monitoring

backend-infrastructure

Backend repos CI

All services, all environments

—

Backend team daily driver

frontend-releases

Frontend repos CI

—

Production + preview

Frontend team deployments

security-monitoring

Security workflows only

Production deployment status

Production only

Security audit dashboard

8.3 Configuration Format

Views are stored in TOML:

# ~/.config/pulsos/config.toml

[[views]]
name = "production"
description = "Production systems across all platforms"
projects = ["my-saas", "api-core", "auth-service"]
platforms = ["github", "railway", "vercel"]
branch_filter = "main"
status_filter = ["success", "failure", "in_progress"]
refresh_interval = 5

[[views]]
name = "frontend"
description = "Frontend deployments"
projects = ["my-saas", "docs-portal"]
platforms = ["github", "vercel"]
vercel_include_previews = true


9. Security & Privacy

9.1 Security Guarantees

Read-only API behavior: Pulsos only performs read operations and never calls write/delete/admin endpoints. GitHub private-repo access currently requires the classic `repo` scope (write-capable token), so this is a platform limitation rather than a Pulsos behavior choice. Prefer GitHub fine-grained PATs with read-only permissions where feasible.

Secure token storage: Tokens are stored in the OS-native keyring (Keychain, Secret Service, Credential Manager). They are never stored in plaintext config files. The fallback encrypted file uses AES-256-GCM with a machine-identifier-derived key plus salt, which is convenience-grade and weaker than keyring-backed storage.

Memory safety: Tokens in memory are wrapped in secrecy::Secret<String>, which zeroes memory on drop. This prevents tokens from appearing in core dumps or memory forensics.

No token logging: Tokens are never written to log files, debug output, or error messages, even in --verbose mode. The secrecy crate's Debug implementation prints [REDACTED] instead of the token value.

File permissions: Config files are created with 0600 permissions (owner read/write only). Pulsos warns if permissions are too open and refuses to read tokens from world-readable files.

Token rotation: pulsos auth status and pulsos doctor warn 7 days before token expiration. pulsos auth refresh guides the user through creating a new token.

Scope validation: On authentication, Pulsos validates that the token has the minimum required scopes and warns if unnecessary write scopes are present (e.g., if a GitHub PAT has delete_repo — Pulsos works but warns the user to create a more restricted token).

9.2 Privacy Model

What Pulsos accesses:

Data

Purpose

Stored Locally?

Repository/project names

Display in dashboard

Yes (config + cache)

Workflow run metadata

CI status display

Yes (cache, TTL-based)

Deployment status

Deployment health display

Yes (cache, TTL-based)

Commit SHAs and messages

Correlation engine

Yes (cache, TTL-based)

Commit author names

Display in detail view

Yes (cache, TTL-based)

Organization/team names

Project discovery

Yes (config)

User email (Railway auth)

Display in auth status

Yes (config)

API tokens

Authentication

Yes (OS keyring, encrypted)

What Pulsos does NOT access:

Source code or file contents

Environment variables or secrets stored in platforms

Billing or payment information

Personal user data beyond what's in commit metadata

Private messages, comments, or PR reviews

Build artifact contents

9.3 Data Residency

All data stored by Pulsos lives exclusively on the user's local machine:

Location

Contents

~/.config/pulsos/config.toml

View definitions, project mappings, preferences

~/.config/pulsos/credentials.enc

Encrypted token fallback (only if no OS keyring)

~/.cache/pulsos/

Cached API responses (sled database)

OS Keyring

API tokens

Pulsos does not phone home, collect telemetry, or transmit any data to Lambda or any third party. The binary communicates only with the three platform APIs that the user explicitly authenticates with.

9.4 Telemetry Policy

Pulsos collects zero telemetry. No usage data, no crash reports, no analytics. This is a deliberate choice for an open-source DevOps tool — users monitoring their infrastructure should not have to worry about their monitoring tool monitoring them.

If Lambda later wants to understand usage patterns, it can add opt-in telemetry with an explicit, off-by-default flag and a clear disclosure. This is deferred indefinitely.

9.5 Supply Chain Security

As a Rust project:

All dependencies are pinned in Cargo.lock (committed to the repository)

Pre-built binaries are built in CI (GitHub Actions) with reproducible builds and published to GitHub Releases with SHA-256 checksums

The Homebrew formula verifies the checksum on download

Dependencies are audited with cargo audit in CI, blocking releases on known vulnerabilities

The project uses cargo deny to enforce license compatibility and block problematic dependencies

9.6 Threat Model

Threat

Mitigation

Token theft from disk

OS keyring with user-session-locked access; encrypted fallback with 0600 permissions

Token theft from memory

secrecy crate zeroes on drop; no logging of token values

Token theft from network

All API calls use TLS 1.2+; Pulsos validates certificates

Malicious config injection

Config parser (TOML) doesn't execute code; no shell expansion

Dependency supply chain attack

cargo audit + cargo deny in CI; pinned Cargo.lock

Stolen pre-built binary

SHA-256 checksums on GitHub Releases; Homebrew formula verification

Overprivileged token

Scope validation on auth; warns about unnecessary write permissions

10. Distribution & Installation

10.1 Pre-Built Binaries (Primary)

Pre-built binaries are the recommended installation method. They require no Rust toolchain and no Node.js.

Platform

Architecture

Download

macOS

Apple Silicon (aarch64)

pulsos-darwin-aarch64.tar.gz

macOS

Intel (x86_64)

pulsos-darwin-x86_64.tar.gz

Linux

x86_64 (GNU)

pulsos-linux-x86_64-gnu.tar.gz

Linux

x86_64 (musl, static)

pulsos-linux-x86_64-musl.tar.gz

Linux

aarch64

pulsos-linux-aarch64-gnu.tar.gz

Windows

x86_64

pulsos-windows-x86_64.zip

All binaries are built in GitHub Actions CI and published to GitHub Releases with SHA-256 checksums.

10.2 Package Managers

# macOS (Homebrew)
brew install lambdahq/tap/pulsos

# macOS/Linux (install script)
curl -fsSL https://pulsos.dev/install.sh | sh

# Windows (Scoop)
scoop bucket add lambdahq https://github.com/lambdahq/scoop-bucket
scoop install pulsos

# From source (requires Rust toolchain)
cargo install pulsos

# AUR (Arch Linux) — community maintained
yay -S pulsos


10.3 CI/CD Installation

For GitHub Actions:

- name: Install Pulsos
  run: |
    curl -fsSL https://pulsos.dev/install.sh | sh
    pulsos auth --ci \
      --github-token "${{ secrets.GITHUB_TOKEN }}" \
      --railway-token "${{ secrets.RAILWAY_TOKEN }}" \
      --vercel-token "${{ secrets.VERCEL_TOKEN }}"
    pulsos status --format json > deployment-status.json


11. Watch Mode & TUI

The --watch flag on pulsos status activates a full-screen terminal UI built with Ratatui. This is the primary real-time monitoring experience for v1.0, replacing the need for a system tray daemon.

The TUI architecture, layout, keybindings, and rendering pipeline are specified in the companion document: Pulsos TUI PRD v1.0.

Key points for this document:

The TUI runs inside the same binary — no separate process or daemon

Cache-first rendering: cached data appears within 200ms of launch, fresh data replaces it asynchronously

Three tabs: Unified Overview, Platform Deep Dive, Health & Metrics

Minimum terminal size: 80×24

Dark theme by default, light theme via --light flag or PULSOS_THEME=light

All navigation is keyboard-only in v1.0 (vim keys + arrow keys)

12. Dependencies

12.1 Rust Crates

Crate

Purpose

Phase

clap (derive)

CLI argument parsing and help generation

1

reqwest + tokio

Async HTTP client for all API calls

1

serde + serde_json

JSON serialization/deserialization

1

toml

Configuration file parsing

1

tabled

Table output formatting (non-TUI mode)

1

sled

Embedded key-value database for cache

1

chrono

Timestamp handling, TTL, relative time

1

keyring

OS-native credential storage

2

secrecy

Token memory zeroing and redaction

2

dialoguer

Interactive prompts and selection

2

fuzzy-matcher

Fuzzy finding for repo/project selection

3

ratatui + crossterm

Terminal UI for watch mode

6

graphql_client

Typed GraphQL queries for Railway API

1

dirs

Cross-platform config/cache directory paths

1

thiserror + anyhow

Error handling

1

12.2 External Dependencies

None. Pulsos is a single, self-contained binary. No gh, railway, vercel, Node.js, Python, or any other runtime is required.

13. Implementation Phases

Phase

Name

Duration

Deliverables

1

Foundation

Weeks 1–2

Core CLI structure (clap), HTTP client setup (reqwest + tokio), GitHub REST adapter, Railway GraphQL adapter, Vercel REST adapter, basic table output, cache layer with sled + TTL, error recovery, offline mode, staleness indicators

2

Authentication

Weeks 3–4

Token-based auth for all three platforms, OS keyring storage (keyring crate), token detection from existing CLIs, auth status dashboard, auth --ci mode, scope validation, token expiration warnings

3

First-Run & Discovery

Week 5

First-run wizard (combined auth + discovery + correlation), repos sync command, GitHub org/repo discovery, Railway workspace/project discovery, Vercel team/project discovery, auto-correlation detection, interactive fuzzy selection, permission verification

4

View System

Week 6

View creation wizard, template system, TOML configuration, view validation, JSON export/import

5

Correlation Engine

Week 7

Commit SHA correlation (GitHub↔Vercel exact match), heuristic correlation (GitHub↔Railway), explicit mapping configuration, confidence indicators, health scoring (0–100)

6

Watch Mode & TUI

Week 8

Ratatui integration, real-time auto-refresh, three-tab layout, sparkline graphs, keyboard navigation, responsive terminal layout, panic recovery

7

Doctor & Polish

Week 9

pulsos doctor (comprehensive diagnostics), shell completions (bash, zsh, fish, PowerShell), man pages, error message quality pass

—

v1.0 Release

Week 10

Internal dogfooding at Lambda, open-source release, documentation, pre-built binaries, Homebrew tap, install script

8

Daemon & Tray (v2.0)

Weeks 11–14

Background daemon, system tray icon, click-to-CLI, desktop notifications, cross-platform testing

14. Success Metrics

Metric

Target

How Measured

Install to first dashboard

< 3 minutes

Time from download to seeing status output (pre-built binary path)

First-run wizard completion

< 5 minutes

Time from pulsos to fully configured with all platforms

pulsos status latency (warm cache)

< 500ms

Time from command to full table render

pulsos status latency (cold, network)

< 3 seconds

Time from command to full table render

Offline mode activation

< 1 second

Time from network loss to cached data display

Cache staleness visibility

100%

Every data point older than TTL shows age

Doctor command coverage

All known failure modes

Every support-worthy issue has a diagnostic check

Binary size

< 15 MB

Compressed release binary

Memory usage (idle)

< 20 MB

pulsos status --watch with 10 projects

Cross-platform support

macOS, Linux, Windows

CI tests passing on all three

15. Open Source Strategy

Pulsos is released under the MIT license. The open-source strategy supports both community adoption and Lambda's positioning as a technically credible engineering organization.

Repository: github.com/lambdahq/pulsos

License: MIT

Contribution model: Open issues and PRs welcome; Lambda maintains core direction and release cadence

Documentation: README, architecture docs, and contributor guide in the repo

Distribution: Pre-built binaries (GitHub Releases), Homebrew tap, Cargo (crates.io), install script, Scoop (Windows)

Pulsos serves as a public demonstration of Lambda's engineering quality. The codebase should exemplify clean Rust, comprehensive error handling, thoughtful CLI design, and thorough documentation. It's a portfolio piece as much as it is a product.

Appendix A: Configuration Reference

# ~/.config/pulsos/config.toml — Complete Reference

# ── Authentication ──────────────────────────────────────
# Tokens are NOT stored here — they live in the OS keyring.
# These settings control auth behavior only.

[auth]
github_host = "github.com"  # For GitHub Enterprise: "github.mycompany.com"

[auth.token_detection]
# Automatically detect tokens from existing CLI installations
detect_gh_cli = true       # Look for ~/.config/gh/hosts.yml
detect_railway_cli = true  # Look for ~/.railway/config.json
detect_vercel_cli = true   # Look for ~/.vercel/auth.json
detect_env_vars = true     # Check GITHUB_TOKEN, RAILWAY_TOKEN, VERCEL_TOKEN

# ── GitHub ──────────────────────────────────────────────

[[github.organizations]]
name = "myorg"
include_patterns = ["my-saas", "api-*", "auth-*"]
exclude_patterns = ["*-legacy", "experiments/*"]
auto_discover = true

[[github.organizations]]
name = "client-work"
include_patterns = ["*"]
exclude_patterns = ["internal-*"]
auto_discover = false

# ── Railway ─────────────────────────────────────────────

[[railway.workspaces]]
name = "lambda-prod"
include_projects = ["my-saas-api", "api-core-prod", "auth-service-prod"]
exclude_projects = []
default_environment = "production"

[[railway.workspaces]]
name = "lambda-staging"
include_projects = ["my-saas-staging"]
default_environment = "staging"

# ── Vercel ──────────────────────────────────────────────

[[vercel.teams]]
name = "lambda"
include_projects = ["my-saas-web", "docs-portal"]
include_preview_deployments = true

# ── Correlations ────────────────────────────────────────

[[correlations]]
name = "my-saas"
github_repo = "myorg/my-saas"
railway_project = "my-saas-api"
railway_workspace = "lambda-prod"
railway_environment = "production"
vercel_project = "my-saas-web"
vercel_team = "lambda"
branch_mapping = { main = "production", develop = "staging" }

[[correlations]]
name = "api-core"
github_repo = "myorg/api-core"
railway_project = "api-core-prod"
railway_workspace = "lambda-prod"

# ── Views ───────────────────────────────────────────────

[[views]]
name = "production"
description = "Production systems"
projects = ["my-saas", "api-core", "auth-service"]
platforms = ["github", "railway", "vercel"]
branch_filter = "main"
status_filter = ["success", "failure", "in_progress"]
refresh_interval = 5

[[views]]
name = "frontend"
description = "Frontend deployments"
projects = ["my-saas", "docs-portal"]
platforms = ["github", "vercel"]
vercel_include_previews = true

# ── TUI Settings ────────────────────────────────────────

[tui]
refresh_interval = 5
fps = 10
theme = "dark"
unicode = "auto"
default_tab = "unified"
show_sparklines = true

# ── Cache Settings ──────────────────────────────────────

[cache]
directory = "~/.cache/pulsos/"
max_size_mb = 100
