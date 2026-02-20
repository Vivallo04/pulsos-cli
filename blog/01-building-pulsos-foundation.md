# One Commit, Three Platforms, Zero Node.js

You push a commit to `main`. GitHub Actions picks it up — CI is green in 2 minutes. Railway starts a build. Vercel spins up a preview deployment. These three things are all about the same commit, but you have no single place to see that. You tab-switch: GitHub's workflow run page, Railway's project dashboard, Vercel's deployment list. Three UIs, three mental models, three refresh cycles.

I got tired of it. So I built `pulsos` — a Rust CLI that pulls from all three APIs and shows you the unified picture in one terminal window.

This post is about the foundation: workspace layout, platform adapters, credential management, and the cache layer. The interesting parts are in the decisions, not just the code.

---

## The Dependency Trap

The original plan was simple: shell out to `gh`, `railway`, and `vercel` CLIs and parse their output. Two days in, I scrapped it.

`gh` is Go — fine. `railway` is Node.js 16+. `vercel` is Node.js with a global npm install. A Rust binary that requires Node.js to function is architecturally incoherent. You'd be distributing a native binary that silently depends on a JavaScript runtime the user may or may not have, at whatever version it happens to be.

The alternative is obvious in retrospect: call the APIs directly. GitHub has a clean REST API. Railway has a GraphQL API. Vercel has REST. No runtime dependencies, no parsing fragile CLI output, no version skew. The binary is self-contained.

The workspace layout reflects this cleanly:

```
pulsos-cli/
  crates/
    pulsos-core/   # library: platform clients, domain types, correlation
    pulsos-cli/    # binary: commands, TUI, output rendering
    pulsos-test/   # shared test fixtures and mock servers
```

`pulsos-core` has no knowledge of the terminal. `pulsos-cli` has no knowledge of HTTP. The boundary is enforced by the workspace.

---

## Three APIs, One Interface

The core abstraction is `PlatformAdapter`:

```rust
pub trait PlatformAdapter: Send + Sync {
    fn fetch_events(
        &self,
        tracked: &[TrackedResource],
    ) -> impl Future<Output = Result<Vec<DeploymentEvent>, PulsosError>> + Send;

    fn discover(&self) -> impl Future<Output = Result<Vec<DiscoveredResource>, PulsosError>> + Send;

    fn validate_auth(&self) -> impl Future<Output = Result<AuthStatus, PulsosError>> + Send;

    fn rate_limit_status(&self) -> impl Future<Output = Result<RateLimitInfo, PulsosError>> + Send;
}
```

Each platform adapter converts its wire format into `DeploymentEvent` — the unified domain type:

```rust
pub struct DeploymentEvent {
    pub id: String,
    pub platform: Platform,           // GitHub | Railway | Vercel
    pub status: DeploymentStatus,     // normalized across all three platforms
    pub commit_sha: Option<String>,   // exact for GitHub/Vercel, absent for Railway
    pub branch: Option<String>,
    pub title: Option<String>,
    pub actor: Option<String>,
    pub created_at: DateTime<Utc>,
    pub duration_secs: Option<u64>,
    pub url: Option<String>,
    pub metadata: EventMetadata,      // platform-specific extras
}
```

The `commit_sha` field is worth noting — the `Option` there is load-bearing. GitHub surfaces it on every workflow run as `head_sha`. Vercel surfaces it as `meta.githubCommitSha`. Railway doesn't surface it at all. That asymmetry is what makes the correlation problem interesting, and we'll come back to it in the next post.

**GitHub** was the easiest. Standard REST, well-documented, full commit SHA on every workflow run. Rate limit headers come back on every response (`X-RateLimit-Remaining`), which we track to warn the user before they hit the wall.

**Railway** uses GraphQL at `backboard.railway.com/graphql/v2`. We use `graphql_client` for codegen — the schema is downloaded once, typed queries are generated at build time, and the compiler catches field-name drift before it becomes a runtime mystery.

**Vercel** has a hidden gem in its REST response: every deployment includes `meta.githubCommitSha`. This is free exact-match correlation with GitHub — no heuristics needed. It also includes `link.repo` on the project object, which tells us the GitHub repo the project is connected to. We use both of these aggressively during setup.

---

## The Credential Puzzle

Three platforms, three token formats, multiple possible storage locations each. The goal: authenticate automatically if the user already has any of the CLIs installed, and fall back to manual entry otherwise.

The detection chain for each platform:

**GitHub** — parse `~/.config/gh/hosts.yml` (or `$GH_CONFIG_DIR/hosts.yml`). The format is YAML, but adding a full YAML parser just for this would be over-engineering. We use a line-by-line parser that looks for the `github.com:` block and extracts `oauth_token`:

```rust
fn parse_gh_hosts_yml(content: &str) -> Option<String> {
    let mut in_github_block = false;

    for line in content.lines() {
        let trimmed = line.trim();

        if trimmed == "github.com:" || trimmed == "\"github.com\":" {
            in_github_block = true;
            continue;
        }

        // Leave block on any unindented non-empty line
        if in_github_block && !line.starts_with(' ') && !line.starts_with('\t') && !trimmed.is_empty() {
            in_github_block = false;
        }

        if in_github_block {
            if let Some(token) = trimmed.strip_prefix("oauth_token:") {
                let token = token.trim().trim_matches('"').trim_matches('\'');
                if !token.is_empty() { return Some(token.to_string()); }
            }
        }
    }
    None
}
```

**Railway** — `~/.railway/config.json`, a simple `{"token": "..."}` object.

**Vercel** — `$XDG_DATA_HOME/com.vercel.cli/auth.json`, with fallback to `~/.vercel/auth.json` for older installs. On macOS, `XDG_DATA_HOME` resolves to `~/Library/Application Support`.

Once detected, tokens are wrapped in `Secret<String>` from the `secrecy` crate. The type zeroes memory on drop and displays as `[REDACTED]` in debug output — which matters when you're building something that might end up in a bug report or log file.

The full resolver chain per platform: OS keyring (macOS Keychain or Linux Secret Service) → environment variable → CLI config file. You can override at any level. The lowest-level source that succeeds wins.

---

## Cache-First Architecture

Deployment APIs are not designed to be hammered. GitHub's REST API is 5,000 requests per hour for authenticated users — generous, but you'll burn through it fast with naive polling across multiple repos. Railway and Vercel have their own limits.

We use `sled` as an embedded key-value store. It's pure Rust, no daemon, no setup. The cache entries carry their own TTL and a three-state freshness model:

```rust
impl<T> CacheEntry<T> {
    pub fn is_fresh(&self) -> bool {
        let age = Utc::now() - self.fetched_at;
        age.num_seconds() < self.ttl_secs as i64
    }

    pub fn is_stale(&self) -> bool {
        !self.is_fresh() && !self.is_expired()
    }

    // Expired = older than TTL * 120 (configurable staleness window)
    pub fn is_expired(&self) -> bool {
        let max_staleness = self.ttl_secs.saturating_mul(STALENESS_MULTIPLIER);
        let age = Utc::now() - self.fetched_at;
        age.num_seconds() > max_staleness as i64
    }
}
```

Fresh data is served from cache without hitting the network. Stale data is still served — with an age indicator in the UI — while a background refresh fires. Expired data triggers a blocking fetch. This means a network hiccup doesn't blank out the dashboard; it shows you the last known state with a timestamp.

The TTL split: 30 seconds for deployment events (things change fast), 6 hours for org/workspace metadata (repo lists, project names — basically static).

---

## The First-Run Moment

The whole point of the tool is zero-config correlation. When you run `pulsos` for the first time, it:

1. Detects whatever tokens are already available
2. Calls `discover()` on each authenticated platform
3. Runs the auto-correlation engine against the discovered resources

The correlation at discovery time works in two tiers. First, Vercel's `link.repo` field gives us free exact matches — if a Vercel project is linked to `myorg/my-saas`, we know immediately that it belongs with that GitHub repo.

For everything else, we use name stem matching. The `name_stem()` function strips common deployment suffixes:

```rust
pub fn name_stem(name: &str) -> &str {
    const SUFFIXES: &[&str] = &[
        "-web", "-api", "-app", "-frontend",
        "-backend", "-service", "-server", "-client", "-worker",
    ];
    for suffix in SUFFIXES {
        if let Some(stem) = name.strip_suffix(suffix) {
            if !stem.is_empty() { return stem; }
        }
    }
    name
}
```

So `my-saas-web` (Vercel), `my-saas-api` (Railway), and `my-saas` (GitHub repo) all reduce to the same stem `my-saas` and get grouped into one unified project. The user confirms the groupings, it gets written to config, and every subsequent run uses that mapping.

Two interactions from zero to a live dashboard. That's the target.

---

## What's Next

The foundation was solid: API clients that normalize three different wire formats into one domain type, a credential chain that works silently for most users, and a cache layer that gracefully degrades under network pressure.

But there's a problem we haven't solved yet. The `commit_sha` field on `DeploymentEvent` is `None` for every Railway event. GitHub knows which commit triggered a workflow run. Vercel knows which commit triggered a deployment. Railway returns an `id`, a `status`, and a timestamp — and nothing else about the underlying commit.

The whole value of the tool is showing you that these three events are about the same commit. Without a SHA on the Railway side, you can't do that with certainty. You have to be smart about it.

That's what the next post is about.
