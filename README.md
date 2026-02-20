# pulsos

[![CI](https://github.com/Vivallo04/pulsos-cli/actions/workflows/ci.yml/badge.svg)](https://github.com/Vivallo04/pulsos-cli/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)

Cross-platform deployment monitoring CLI. Track deployments across GitHub Actions, Railway, and Vercel from a single terminal.

## What is Pulsos

Pulsos is a terminal tool that gives you a unified view of deployments across GitHub Actions, Railway, and Vercel. It runs as a live TUI dashboard or as one-shot CLI output. The correlation engine matches deployment events across platforms by commit SHA and timestamp heuristics, so you can see how a single commit flows through your CI/CD pipeline.

## Features

- **Live TUI** with 5 tabs: Unified, Platform, Health, Settings, Logs
- **Cross-platform correlation** — matches events by SHA and timestamp proximity
- **Health scoring** per project (weighted: GitHub 40%, Railway 35%, Vercel 25%)
- **Auto-discovery** of repos, projects, and services via `repos sync`
- **Saved views** with filters for project, platform, branch, and status
- **ETag caching** with rate-limit-aware adaptive polling
- **Credential storage** — OS keyring with file fallback (`~/.config/pulsos/credentials.toml`)
- **Output formats** — table, compact, JSON
- **Shell completions** for bash, zsh, fish, powershell, elvish

## Install

### From crates.io

```sh
cargo install pulsos-cli
```

### From source

```sh
git clone https://github.com/Vivallo04/pulsos-cli.git
cd pulsos-cli
cargo install --path crates/pulsos-cli
```

### Homebrew (macOS / Linux)

```sh
brew install Vivallo04/tap/pulsos
```

### Requirements

- Rust 1.75+

## Quick Start

```sh
# 1. Authenticate with your platforms
pulsos auth github
pulsos auth railway
pulsos auth vercel

# 2. Discover and track your repos/projects
pulsos repos sync

# 3. Launch the live TUI dashboard
pulsos status --watch

# Or get a one-shot table in your terminal
pulsos status --once
```

**What happens at each step:**

1. `pulsos auth <platform>` prompts for a token and stores it in your OS keyring (falls back to `~/.config/pulsos/credentials.toml`).
2. `pulsos repos sync` queries each authenticated platform, discovers available repos/projects/services, lets you pick which ones to track, and auto-generates correlations.
3. `pulsos status` fetches recent deployments from all tracked resources, correlates them, and displays the result. With `--watch` it launches the live TUI; without flags it auto-detects (TUI if interactive terminal, one-shot otherwise).

## Authentication

### Token Resolution Order

Pulsos resolves tokens in this priority order:

1. **Environment variables** — checked first
2. **OS keyring** — via `keyring` crate (macOS Keychain, Windows Credential Manager, Linux Secret Service)
3. **CLI config detection** — reads tokens from `gh`, `railway`, and `vercel` CLI config files

### Environment Variables

| Platform | Variables |
|----------|-----------|
| GitHub   | `GH_TOKEN`, `GITHUB_TOKEN` |
| Railway  | `RAILWAY_TOKEN`, `RAILWAY_API_TOKEN` |
| Vercel   | `VERCEL_TOKEN` |

### Interactive Setup

```sh
pulsos auth github              # Authenticate with GitHub
pulsos auth railway             # Authenticate with Railway
pulsos auth vercel              # Authenticate with Vercel
```

Each command prompts for a token, validates it against the platform API, and stores it.

### Non-Interactive / CI

```sh
# Store tokens in one shot (CI mode)
pulsos auth --ci --github-token=ghp_xxx --railway-token=xxx --vercel-token=xxx

# Check which tokens are resolved
pulsos auth status

# Re-validate stored tokens; re-prompts if invalid
pulsos auth refresh

# Remove stored tokens
pulsos auth logout
pulsos auth logout --platform github
```

### Token Detection Toggles

You can disable automatic detection of specific sources in `config.toml`:

```toml
[auth.token_detection]
detect_gh_cli = true
detect_railway_cli = true
detect_vercel_cli = true
detect_env_vars = true
```

## Configuration

Config file location: `~/.config/pulsos/config.toml`

Run `pulsos config path` to print the exact path, or `pulsos config edit` to open it in `$EDITOR`.

### Full Example

```toml
[auth]
github_host = "github.mycompany.com"   # Default: "github.com"

[auth.token_detection]
detect_gh_cli = true
detect_railway_cli = false
detect_vercel_cli = true
detect_env_vars = true

[[github.organizations]]
name = "myorg"
include_patterns = ["api-*"]
exclude_patterns = ["*-legacy"]
auto_discover = true

[[railway.workspaces]]
name = "lambda-prod"
include_projects = ["my-saas-api"]
default_environment = "production"      # Default: "production"

[[vercel.teams]]
name = "lambda"
include_projects = ["my-saas-web"]
include_preview_deployments = true

[[correlations]]
name = "my-saas"
github_repo = "myorg/my-saas"
railway_project = "my-saas-api"
vercel_project = "my-saas-web"

[correlations.branch_mapping]
main = "production"
develop = "staging"

[[views]]
name = "production"
description = "Production systems"
projects = ["my-saas", "api-core"]
platforms = ["github", "railway", "vercel"]
branch_filter = "main"
status_filter = ["success", "failure"]
refresh_interval = 5

[tui]
theme = "dark"                  # "dark" or "light"
fps = 10                        # Render frames per second
refresh_interval = 5            # Seconds between API polls
default_tab = "unified"         # "unified", "platform", "health", "settings", "logs"
show_sparklines = true
unicode = "auto"                # "auto", "always", "never"

[cache]
max_size_mb = 100
```

### Config Sections

| Section | Purpose |
|---------|---------|
| `[auth]` | GitHub Enterprise host, token detection toggles |
| `[[github.organizations]]` | Org name, include/exclude repo patterns |
| `[[railway.workspaces]]` | Workspace name, include/exclude projects, default environment |
| `[[vercel.teams]]` | Team name, include projects, preview deployment toggle |
| `[[correlations]]` | Link a GitHub repo, Railway project, and Vercel project under one name |
| `[[views]]` | Named filter presets (projects, platforms, branch, status) |
| `[tui]` | Theme, FPS, refresh interval, default tab, sparklines |
| `[cache]` | Max cache size in MB |

## CLI Commands

### Status

```sh
pulsos                              # Default: show deployment status
pulsos status                       # Same as above
pulsos status --watch               # Live TUI mode
pulsos status --once                # Force one-shot output
pulsos status --platform github     # Filter by platform
pulsos status --view production     # Use a saved view
pulsos status --branch main         # Filter by branch
pulsos status --format json         # JSON output
pulsos status --format compact      # Compact output
```

### Auth

```sh
pulsos auth github                  # Authenticate with GitHub
pulsos auth railway                 # Authenticate with Railway
pulsos auth vercel                  # Authenticate with Vercel
pulsos auth github --token ghp_xxx  # Non-interactive with token
pulsos auth status                  # Check auth status across platforms
pulsos auth refresh                 # Re-validate tokens, re-prompt if invalid
pulsos auth refresh --platform github  # Refresh a specific platform
pulsos auth logout                  # Remove tokens (prompts for platform)
pulsos auth logout --platform github   # Remove a specific platform token
pulsos auth --ci --github-token=xxx    # CI mode: store tokens non-interactively
pulsos auth --from-env              # Only check env vars (skip keyring/interactive)
```

### Repos

```sh
pulsos repos sync                   # Discover, select, and save (all platforms)
pulsos repos list                   # Show tracked repos/projects
pulsos repos add github:org/repo    # Add a resource
pulsos repos remove github:org/repo # Remove a resource
pulsos repos correlate my-saas      # Edit correlations for a project
pulsos repos groups list            # List resource groups
pulsos repos groups create mygroup -- github:org/repo railway:project
pulsos repos groups delete mygroup  # Delete a group
pulsos repos verify                 # Check access permissions for tracked resources
```

### Views

```sh
pulsos views                        # List all views (default)
pulsos views list                   # List all views
pulsos views show production        # Display view details
pulsos views create                 # Create a view interactively
pulsos views edit production        # Edit a view interactively
pulsos views delete production      # Delete a view
pulsos views templates              # List built-in templates
pulsos views validate production    # Validate view projects against correlations
pulsos views export production -o view.json  # Export to JSON
pulsos views import view.json       # Import from JSON
```

### Config

```sh
pulsos config                       # Print current config as TOML (default)
pulsos config show                  # Print current config as TOML
pulsos config path                  # Print config file path
pulsos config edit                  # Open config in $EDITOR
pulsos config wizard                # Run interactive platform setup wizard
```

### Other

```sh
pulsos doctor                       # Run diagnostics (system, auth, connectivity, cache)
pulsos completions bash             # Generate shell completions
pulsos completions zsh
pulsos completions fish
pulsos completions powershell
pulsos completions elvish
```

### Global Flags

| Flag | Description |
|------|-------------|
| `--format <table\|compact\|json>` | Output format (default: table) |
| `--no-color` | Disable color output |
| `--verbose` | Show debug information |
| `--config <path>` | Custom config file path |

## TUI Keyboard Shortcuts

| Key | Action |
|-----|--------|
| `1`–`5` | Switch to tab (Unified, Platform, Health, Settings, Logs) |
| `Tab` / `Shift+Tab` | Cycle tabs forward / backward |
| `j` / `k` or `↑` / `↓` | Navigate rows |
| `/` | Enter search mode |
| `Enter` | Apply search (in search mode) |
| `Esc` | Exit search mode / cancel |
| `r` | Force refresh |
| `q` / `Ctrl+C` | Quit |

### Settings Tab

| Key | Action |
|-----|--------|
| `t` | Enter token for selected platform |
| `T` | Enter token (force override even with env token) |
| `v` | Validate selected platform |
| `x` | Remove stored token |
| `o` / `Enter` | Start onboarding / discovery flow |

## Project Architecture

```
pulsos-cli/
├── crates/
│   ├── pulsos-core/          # Library crate
│   │   └── src/
│   │       ├── platform/     # GitHub, Railway, Vercel API clients
│   │       ├── correlation/  # Event matching engine (SHA + timestamp heuristic)
│   │       ├── domain/       # DeploymentEvent, CorrelatedEvent, health scoring
│   │       ├── auth/         # Credential resolution, keyring, file store
│   │       ├── config/       # TOML config loading and saving
│   │       ├── cache/        # sled-based ETag cache
│   │       ├── health/       # Platform health checks
│   │       ├── scheduler/    # Polling budget and adaptive scheduler
│   │       └── sync/         # Auto-correlation builder
│   ├── pulsos-cli/           # Binary crate
│   │   └── src/
│   │       ├── main.rs       # CLI entry point (clap)
│   │       ├── commands/     # Command handlers (status, auth, repos, views, config, doctor)
│   │       ├── tui/          # ratatui TUI (poll, render, keys, widgets, settings flow)
│   │       └── output/       # Table, compact, JSON formatters
│   └── pulsos-test/          # Test helpers and builders
└── Cargo.toml                # Workspace root
```

## Development

### Prerequisites

- Rust 1.75+

### Build

```sh
cargo build --workspace
```

### Test

```sh
cargo test --workspace
```

### Lint

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
```

### Run Locally

```sh
cargo run --bin pulsos -- status
cargo run --bin pulsos -- status --watch
cargo run --bin pulsos -- doctor
```

## License

MIT
