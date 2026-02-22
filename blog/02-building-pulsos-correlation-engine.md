# The Hardest Part: Teaching Railway to Speak Git

You push `abc1234` to `main`.

GitHub knows it. The workflow run object has a `head_sha` field — `abc1234`. Vercel knows it. The deployment response includes `meta.githubCommitSha` — `abc1234`. You can match these two events with a string comparison. Zero ambiguity.

Railway? The deployment object has an `id`, a `status`, a `createdAt`. No SHA. No branch name in the API response. Nothing that lets you say "this Railway deployment corresponds to commit `abc1234`."

This is the one problem the whole tool hinges on. If you can't connect Railway to the rest of the picture, the unified view is incomplete for anyone running Railway. And everyone running Railway is running it alongside GitHub Actions.

---

## When SHA Matching Is Free

Before getting to the hard part, let's be clear about what works perfectly.

GitHub and Vercel both expose commit SHAs on every event. The match is deterministic: if two events share the same SHA, they're the same commit. The `find_sha_matches` function implements this with one additional constraint — a `claimed_b` array that prevents a single Vercel deployment from being matched to two different GitHub runs:

```rust
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
            if claimed_b[bi] { continue; }
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
```

The result is `Confidence::Exact` — the highest tier in the confidence system. When you see `● Exact` in the TUI, it means both a GitHub workflow run and a Vercel deployment reported the same commit SHA.

---

## Railway's SHA Blindspot and the Heuristic Solution

Railway's API omission isn't a bug — it's a design choice that makes sense for their use case. Railway deployments can be triggered from their dashboard, from webhooks, from the CLI, from automatic triggers on service configuration changes. Not all of those have a meaningful commit SHA. So the API doesn't surface one.

For pulsos, this means heuristics are the only option. The PRD is explicit about this: we're being honest about uncertainty, not pretending we have information we don't. The confidence system exists precisely because we need to surface Railway deployments while communicating that the link is inferred, not proven.

The heuristic: if a Railway deployment starts within 120 seconds of a GitHub workflow completing on the same project, they're probably about the same commit. "Probably" is doing real work in that sentence.

The implementation is `find_closest_by_timestamp`. The key design decision here is *nearest match*, not *first match*. If two Railway deployments both fall within the window, we want the one whose timestamp is closest to the GitHub run — not whichever one happens to be first in the list:

```rust
pub const TIMESTAMP_WINDOW_SECS: i64 = 120;

pub fn find_closest_by_timestamp(
    reference: &DeploymentEvent,
    candidates: &[&DeploymentEvent],
    claimed: &[bool],
    window_secs: i64,
) -> Option<HeuristicMatch> {
    let ref_ts = reference.created_at.timestamp();
    let mut best: Option<HeuristicMatch> = None;

    for (i, candidate) in candidates.iter().enumerate() {
        if claimed[i] { continue; }

        let diff = (candidate.created_at.timestamp() - ref_ts).abs();
        if diff > window_secs { continue; }

        match &best {
            Some(current_best) if diff >= current_best.time_diff_secs => {}
            _ => {
                best = Some(HeuristicMatch { candidate_index: i, time_diff_secs: diff });
            }
        }
    }

    best
}
```

The `claimed` parameter does the same job as `claimed_b` in the SHA matcher: once a Railway deployment has been assigned to one GitHub run, it can't be assigned to another. This matters when you have multiple rapid deploys — you need a bijection, not a many-to-one mapping.

---

## The Confidence Scoring System

The `Confidence` enum has four levels, derived with `Ord` so they sort naturally from weakest to strongest:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Confidence {
    Unmatched,  // no correlation
    Low,        // timestamp-only heuristic
    High,       // timestamp + explicit config mapping
    Exact,      // SHA match (GitHub ↔ Vercel only)
}

impl fmt::Display for Confidence {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Exact     => write!(f, "● Exact"),
            Self::High      => write!(f, "◐ High"),
            Self::Low       => write!(f, "○ Low"),
            Self::Unmatched => write!(f, "? Unmatched"),
        }
    }
}
```

The scoring logic is a pure function — four cases, no side effects:

```rust
pub fn score_confidence(
    sha_matched: bool,
    timestamp_matched: bool,
    has_explicit_mapping: bool,
) -> Confidence {
    if sha_matched {
        Confidence::Exact
    } else if timestamp_matched && has_explicit_mapping {
        Confidence::High
    } else if timestamp_matched {
        Confidence::Low
    } else {
        Confidence::Unmatched
    }
}
```

The `has_explicit_mapping` flag distinguishes `High` from `Low`. If the user has configured `railway_project = "rw-proj-1"` in their correlation config, we have explicit intent: they've told us this Railway project belongs with this GitHub repo. A timestamp match on top of that explicit mapping earns `High`. A timestamp match with no explicit mapping — maybe we're correlating a Railway project we found by name-stem during auto-discovery — stays `Low`.

The visual indicators `● ◐ ○ ?` matter more than the labels. Users scan dashboards; they don't read labels. An honest `○ Low` next to a Railway deployment is better than a false-confident indicator that would erode trust the first time it's wrong.

---

## The Algorithm: All Three Platforms

The `correlate_project_events` function orchestrates the full pass. The algorithm is documented in the module comment, but the key insight is the ordering: SHA matches first, then heuristic matching of what's left:

```rust
// Step 1: SHA-match GitHub <-> Vercel -> Exact
let sha_matches = find_sha_matches(&github, &vercel);
for (gi, vi) in &sha_matches {
    claimed_github[*gi] = true;
    claimed_vercel[*vi] = true;

    // Try to pull in a Railway deployment for this SHA group
    let railway_event = find_closest_by_timestamp(
        github[*gi], &railway, &claimed_railway, TIMESTAMP_WINDOW_SECS,
    ).map(|m| {
        claimed_railway[m.candidate_index] = true;
        railway[m.candidate_index].clone()
    });

    result.push(CorrelatedEvent {
        commit_sha: github[*gi].commit_sha.clone(),
        github: Some(github[*gi].clone()),
        railway: railway_event,
        vercel: Some(vercel[*vi].clone()),
        confidence: Confidence::Exact, // SHA is the primary signal
        timestamp,
    });
}

// Step 2: Unmatched GitHub events — try heuristic against Railway
// Step 3: Unmatched Vercel events — try heuristic against Railway
// Step 4: Remaining Railway events — standalone Unmatched
```

The three claimed arrays (`claimed_github`, `claimed_railway`, `claimed_vercel`) flow through all four steps. An event claimed in step 1 cannot be claimed again in step 2. This is what prevents a single Railway deployment from appearing twice in the output when it temporally overlaps with both a GitHub and an orphaned Vercel event.

---

## From Inline Hack to Core Module

The correlation engine didn't start this clean. The first working version had `correlate_events()` living inside `tui/poll.rs` — the background polling task. It grouped events by `commit_sha` and didn't handle Railway at all (since Railway events have no SHA, they just fell through). Railway deployments showed up as unmatched orphans every time.

The refactor moved correlation into `pulsos-core` as a proper module: `correlation/mod.rs`, `correlation/sha_match.rs`, `correlation/heuristic.rs`, `correlation/confidence.rs`. Pure functions, no I/O, fully unit tested. The poller went from 165 lines of inline logic to two function calls:

```rust
let correlated = correlation::correlate_all(&config.correlations, &all_events);
let health_scores = health::compute_project_health_scores(&config.correlations, &all_events);
```

The test suite for the correlation engine ended up at 30 unit tests plus 7 integration tests covering multi-project configs, orphaned events, and duplicate SHA edge cases. That coverage is what made the refactor safe — we could move fast without breaking the grouping logic.

---

## The TUI: Real-Time Without Blocking

The TUI uses `ratatui` for rendering and `tokio` for the async runtime. The architecture is a clean producer-consumer split:

- A background `tokio` task (the poller) runs the platform fetches on a throttled schedule
- It sends `DataSnapshot` updates through a `tokio::sync::watch::Sender`
- The renderer reads from the `watch::Receiver` on each frame — always the latest snapshot, never blocking

The throttle intervals are conservative by design:

```rust
const GITHUB_THROTTLE: Duration = Duration::from_secs(30);
const RAILWAY_THROTTLE: Duration = Duration::from_secs(15);
const VERCEL_THROTTLE: Duration = Duration::from_secs(15);
```

GitHub gets a longer interval because its rate limit is per-token across all requests, and users might be watching multiple repos. Railway and Vercel get 15s because their rate limits are less of a concern and deployments move faster.

The health score computation is weighted across platforms:

```rust
pub fn compute(
    github_runs: &[DeploymentStatus],
    railway_status: Option<DeploymentStatus>,
    vercel_status: Option<DeploymentStatus>,
) -> u8 {
    // GitHub CI success rate (last 10 runs): 40%
    // Railway latest deployment status:      35%
    // Vercel latest deployment status:       25%
    //
    // If a platform is not connected, its weight is redistributed
    // proportionally to the connected platforms.
}
```

The 40/35/25 split reflects the signal quality: GitHub CI runs give you a rate over time (more stable signal), Railway and Vercel give you the latest binary status. If a platform isn't connected, its weight redistributes proportionally — a two-platform setup still produces a meaningful 0-100 score.

Health history is stored in a ring buffer capped at 20 entries per project, feeding the sparklines in the TUI's Health tab. 20 entries at a 30-second poll interval is 10 minutes of history — enough to see a trend without consuming unbounded memory.

---

## Doctor: The Support Ticket Killer

Almost every CLI tool failure is one of three things: auth is broken, the network can't reach the API, or the config doesn't match reality. `pulsos doctor` checks all of these and returns actionable output, not raw error dumps:

```
Authentication
  GitHub     ✓ @vivallo (scopes: repo, read:org)
  Railway    ✓ v@lambda.co
  Vercel     ✓ lambda-team

API Reachability
  GitHub     ✓ 4823ms    rate: 4,912/5,000 remaining
  Railway    ✓ 291ms
  Vercel     ✓ 183ms

Correlation Quality
  my-saas    GitHub ✓  Railway ◐ High  Vercel ✓
  api-core   GitHub ✓  Railway ?       Vercel -
```

The Railway `◐ High` on `my-saas` means explicit mapping in config plus recent timestamp matches. The `?` on `api-core` means Railway events are appearing but not matching within the 120-second window — usually a config name mismatch or a slow Railway build pipeline.

Surfacing correlation quality in doctor closes the loop: instead of "Railway deployments not showing up," you get "here's what the engine sees, here's why it's unmatched, here's what to check."

---

## Where Things Stand

Pulsos is in internal dogfooding at Lambda — used daily across the team for deployment monitoring. The tool is a portfolio piece as much as it is a product. The code is meant to demonstrate what a systems-quality Rust CLI looks like: clear workspace separation, domain types that enforce invariants at compile time, a test suite that covers the tricky edge cases, and honest uncertainty in the UI when the data doesn't support confidence.

The Railway SHA gap is real, it's documented, and the heuristic handles it well in practice. The 120-second window catches the vast majority of deploys that follow a CI run. When it misses — usually because a manual Railway deploy happened hours after the last CI run — you get `? Unmatched`, which is the correct answer.

The source will be released after the dogfooding period wraps up.
