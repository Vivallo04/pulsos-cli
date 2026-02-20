# Pulsos TUI — Developer Design Guide

> Senior Apple Design → Ratatui implementation spec.  
> Every decision in this doc maps 1:1 to ratatui widgets, constraints, and color values.  
> Current CLI color palette is preserved throughout.

---

## 0. Foundations

### Terminal rendering contract

Ratatui renders into a `Buffer` of `Cell`s — each cell holds one character (or one wide unicode glyph = 2 cells) plus foreground/background `Color` and `Modifier` flags. There are no fonts, no SVGs, no sub-pixel rendering. Every design decision must resolve to:

```
(char or &str) + Color::Rgb(r,g,b) + Modifier::{BOLD | DIM | ITALIC | UNDERLINED}
```

### Color palette (keep current CLI colors)

Map the existing CLI palette to named constants. Define these once in `crates/pulsos-cli/src/tui/theme.rs`:

```rust
use ratatui::style::Color;

pub struct Theme;

impl Theme {
    // Background layers
    pub const BG_BASE:    Color = Color::Rgb(10,  10,  11);   // #0a0a0b  — terminal bg
    pub const BG_SURFACE: Color = Color::Rgb(17,  17,  19);   // #111113  — panels, headers
    pub const BG_RAISED:  Color = Color::Rgb(24,  24,  27);   // #18181b  — selected rows
    pub const BG_OVERLAY: Color = Color::Rgb(31,  31,  35);   // #1f1f23  — tooltips

    // Text
    pub const TEXT_HI:    Color = Color::Rgb(250, 250, 250);  // #fafafa  — primary
    pub const TEXT_MID:   Color = Color::Rgb(161, 161, 170);  // #a1a1aa  — secondary
    pub const TEXT_LO:    Color = Color::Rgb(113, 113, 122);  // #71717a  — muted
    pub const TEXT_GHOST: Color = Color::Rgb(82,  82,  91);   // #52525b  — disabled

    // Borders
    pub const BORDER:     Color = Color::Rgb(39,  39,  42);   // #27272a
    pub const BORDER_SUB: Color = Color::Rgb(28,  28,  31);   // #1c1c1f

    // Semantic — these match the current green/red/yellow in use
    pub const PASS:       Color = Color::Rgb(16,  185, 129);  // #10b981  emerald
    pub const FAIL:       Color = Color::Rgb(244, 63,  94);   // #f43f5e  rose
    pub const WARN:       Color = Color::Rgb(245, 158, 11);   // #f59e0b  amber
    pub const INFO:       Color = Color::Rgb(59,  130, 246);  // #3b82f6  blue
    pub const CANCEL:     Color = Color::Rgb(82,  82,  91);   // #52525b  slate

    // Platform accent colors
    pub const GH:         Color = Color::Rgb(226, 232, 240);  // near-white
    pub const RW:         Color = Color::Rgb(167, 139, 250);  // violet
    pub const VC:         Color = Color::Rgb(56,  189, 248);  // sky blue
}
```

### Shared style helpers

```rust
// src/tui/theme.rs  (continued)
use ratatui::style::{Modifier, Style};

impl Theme {
    pub fn pass()   -> Style { Style::default().fg(Self::PASS) }
    pub fn fail()   -> Style { Style::default().fg(Self::FAIL) }
    pub fn warn()   -> Style { Style::default().fg(Self::WARN) }
    pub fn cancel() -> Style { Style::default().fg(Self::CANCEL) }
    pub fn muted()  -> Style { Style::default().fg(Self::TEXT_LO) }
    pub fn hi()     -> Style { Style::default().fg(Self::TEXT_HI).add_modifier(Modifier::BOLD) }
    pub fn dim()    -> Style { Style::default().fg(Self::TEXT_GHOST) }
    pub fn info()   -> Style { Style::default().fg(Self::INFO) }

    pub fn selected_row() -> Style {
        Style::default().bg(Self::BG_RAISED).fg(Self::TEXT_HI)
    }
    pub fn header_row() -> Style {
        Style::default().bg(Self::BG_SURFACE).fg(Self::TEXT_GHOST)
            .add_modifier(Modifier::BOLD)
    }
}
```

### Unicode budget

Only use characters that survive on all common terminal emulators (iTerm2, WezTerm, Alacritty, macOS Terminal, Windows Terminal):

| Purpose | Character | Notes |
|---|---|---|
| Pass | `●` U+25CF | Filled circle |
| Fail | `✕` U+2715 | Multiplication X |
| Cancel | `○` U+25CB | Empty circle |
| Running | `◐` U+25D0 | Half circle |
| Pending | `◌` U+25CC | Dotted circle |
| Branch | `⎇` U+2387 | Alternate key |
| Separator | `│` U+2502 | Box vertical |
| Arrow | `›` U+203A | Single right angle |
| Warning | `!` ASCII | Universal |
| Bullet | `·` U+00B7 | Middle dot |
| Spark bars | `▁▂▃▄▅▆▇█` | Block elements |

---

## 1. Global Layout

### Frame decomposition

Every frame renders this top-level split:

```
┌─────────────────────────────────────────────────────────────┐
│  TITLEBAR   36px tall (2 rows)                              │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│  TAB CONTENT  (fills remaining height)                      │
│                                                             │
├─────────────────────────────────────────────────────────────┤
│  STATUSBAR  24px tall (1 row)                               │
└─────────────────────────────────────────────────────────────┘
```

```rust
// src/tui/render.rs
use ratatui::layout::{Constraint, Direction, Layout};

pub fn root_layout(area: Rect) -> [Rect; 3] {
    Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2),   // titlebar
            Constraint::Min(0),      // content
            Constraint::Length(1),   // statusbar
        ])
        .split(area)
        .try_into()
        .unwrap()
}
```

### Titlebar layout

The titlebar row is itself split horizontally:

```
 PULSOS │ Unified 1 │ Platform 2 │ Health 3 │ Settings 4 │ Logs 5    GH! RW✓ VC~  just now
```

```rust
pub fn titlebar_layout(area: Rect) -> [Rect; 3] {
    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(10),  // wordmark "P U L S O S"
            Constraint::Min(0),      // tab bar (fills)
            Constraint::Length(32),  // platform pills + sync status
        ])
        .split(area)
        .try_into()
        .unwrap()
}
```

**Wordmark rendering:**

```rust
use ratatui::text::{Line, Span};

fn render_wordmark() -> Line<'static> {
    Line::from(vec![
        Span::styled("P", Style::default().fg(Theme::PASS).add_modifier(Modifier::BOLD)),
        Span::styled("ULSOS", Style::default().fg(Theme::TEXT_HI).add_modifier(Modifier::BOLD)),
        Span::raw(" "),
    ])
}
```

**Tab bar — use ratatui's `Tabs` widget:**

```rust
use ratatui::widgets::Tabs;

let tab_titles = vec!["Unified 1","Platform 2","Health 3","Settings 4","Logs 5"];

let tabs = Tabs::new(tab_titles)
    .select(app.active_tab as usize)
    .style(Style::default().fg(Theme::TEXT_LO))
    .highlight_style(
        Style::default()
            .fg(Theme::TEXT_HI)
            .add_modifier(Modifier::BOLD)
            .underlined()           // active tab underline
    )
    .divider(Span::styled(" │ ", Style::default().fg(Theme::BORDER)));

frame.render_widget(tabs, titlebar_areas[1]);
```

> **Note on underline color:** Ratatui 0.27+ supports `underline_color()`. Use `.underline_color(Theme::PASS)` to get the emerald underline on the active tab.

**Platform pills (right side):**

```rust
fn render_platform_pills(app: &App) -> Line<'static> {
    let gh = platform_pill("GH", app.github_state);
    let rw = platform_pill("RW", app.railway_state);
    let vc = platform_pill("VC", app.vercel_state);
    Line::from(vec![gh, Span::raw(" "), rw, Span::raw(" "), vc,
        Span::raw("  "),
        Span::styled("just now", Style::default().fg(Theme::TEXT_GHOST)),
    ])
}

fn platform_pill(label: &'static str, state: PlatformState) -> Span<'static> {
    let (symbol, color) = match state {
        PlatformState::Ok          => ("✓", Theme::PASS),
        PlatformState::NeedsConfig => ("!", Theme::WARN),
        PlatformState::ConnError   => ("~", Theme::WARN),
        PlatformState::Unknown     => ("?", Theme::CANCEL),
    };
    Span::styled(
        format!("{}{} ", label, symbol),
        Style::default().fg(color).add_modifier(Modifier::BOLD),
    )
}
```

---

## 2. Statusbar

Single row at the bottom. Three zones:

```
 [q] quit  [Tab] switch tab  [↑↓] navigate  [/] search  [r] refresh   │  WRN  message...
```

```rust
fn render_statusbar(frame: &mut Frame, area: Rect, app: &App) {
    let [shortcuts, _, warn_zone] = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Min(0),
            Constraint::Length(3),   // spacer
            Constraint::Length(55),  // warn pill
        ])
        .split(area)
        .try_into().unwrap();

    // Shortcuts
    let shortcuts_line = Line::from(shortcuts_for_tab(app.active_tab));
    frame.render_widget(Paragraph::new(shortcuts_line)
        .style(Style::default().bg(Theme::BG_SURFACE)), shortcuts);

    // Warn pill — if there's an active warning
    if let Some(msg) = &app.last_warn {
        let warn_text = Line::from(vec![
            Span::styled(" WRN ", Style::default()
                .fg(Theme::WARN).add_modifier(Modifier::BOLD)),
            Span::styled(truncate(msg, 46), Style::default().fg(Theme::TEXT_MID)),
        ]);
        frame.render_widget(
            Paragraph::new(warn_text).style(Style::default().bg(Theme::BG_SURFACE)),
            warn_zone,
        );
    }
}

fn shortcut(key: &'static str, label: &'static str) -> Vec<Span<'static>> {
    vec![
        Span::raw(" "),
        Span::styled(format!("[{}]", key),
            Style::default().fg(Theme::TEXT_HI).add_modifier(Modifier::BOLD)),
        Span::raw(" "),
        Span::styled(label, Style::default().fg(Theme::TEXT_GHOST)),
        Span::raw("  "),
    ]
}

fn shortcuts_for_tab(tab: Tab) -> Vec<Span<'static>> {
    let mut spans = vec![];
    spans.extend(shortcut("q",   "quit"));
    spans.extend(shortcut("Tab", "switch tab"));
    spans.extend(shortcut("↑↓",  "navigate"));
    spans.extend(shortcut("/",   "search"));
    spans.extend(shortcut("r",   "refresh"));
    match tab {
        Tab::Settings => spans.extend(shortcut("t/T", "token")),
        Tab::Health   => spans.extend(shortcut("↵",   "detail")),
        _             => {}
    }
    spans
}
```

---

## 3. Tab 1 — Unified

### Concept

Commit-centric. Each row = one deployment event. You see the project, the SHA, a truncated commit message, and then per-platform status columns. This is the Vercel deployments list grammar applied to all platforms at once.

### Layout

Full-width table, no split panes.

```
 Project        Commit   Message                          GitHub CI   Railway   Vercel   Branch        Age
 ─────────────────────────────────────────────────────────────────────────────────────────────────────────
 tasuky         a3f1b9c  feat: Update role handling…      —           ● passed  —        —             10h
 tasuky         9c2e441  feat: Update CORS policy…        —           ○ cancel  —        —             18h
 pulsos-cli     0662588  Foundation CI setup              ✕ failed    —         —        ⎇ development  1d
 pulsos-cli     2092573  Foundation — CI (pull_request)   ● passed    —         —        ⎇ development  3d
 treetop-cli    f3c9a21  Security scan (scheduled)        ● passed    —         —        ⎇ main         4d
```

### Column widths

```rust
fn unified_column_widths() -> Vec<Constraint> {
    vec![
        Constraint::Length(16),   // project
        Constraint::Length(9),    // sha
        Constraint::Min(24),      // message (fills)
        Constraint::Length(14),   // github ci
        Constraint::Length(12),   // railway
        Constraint::Length(10),   // vercel
        Constraint::Length(18),   // branch
        Constraint::Length(7),    // age
    ]
}
```

### Table rendering

```rust
use ratatui::widgets::{Table, Row, Cell};

fn build_unified_table(events: &[CorrelatedEvent]) -> Table<'_> {
    let header = Row::new(vec![
        Cell::from("PROJECT"),
        Cell::from("COMMIT"),
        Cell::from("MESSAGE"),
        Cell::from("GITHUB CI"),
        Cell::from("RAILWAY"),
        Cell::from("VERCEL"),
        Cell::from("BRANCH"),
        Cell::from("AGE"),
    ]).style(Theme::header_row()).height(1);

    let rows: Vec<Row> = events.iter().map(|e| {
        Row::new(vec![
            Cell::from(Span::styled(&e.project, Theme::hi())),
            Cell::from(Span::styled(sha7(&e.sha), Theme::info())),
            Cell::from(Span::styled(truncate(&e.message, 32), Theme::muted())),
            Cell::from(status_badge(e.github_status)),
            Cell::from(status_badge(e.railway_status)),
            Cell::from(status_badge(e.vercel_status)),
            Cell::from(branch_span(&e.branch)),
            Cell::from(Span::styled(age_str(e.age), Theme::muted())),
        ]).height(1)
    }).collect();

    Table::new(rows, unified_column_widths())
        .header(header)
        .row_highlight_style(Theme::selected_row())
        .highlight_symbol("▶ ")  // or "" for no symbol
}
```

### Status badge helper

This is used across all tabs — put it in `src/tui/widgets/status.rs`:

```rust
pub fn status_badge(status: Option<DeployStatus>) -> Span<'static> {
    match status {
        None => Span::styled("—", Style::default().fg(Theme::TEXT_GHOST)),
        Some(DeployStatus::Pass)    =>
            Span::styled("● passed",    Style::default().fg(Theme::PASS)),
        Some(DeployStatus::Fail)    =>
            Span::styled("✕ failed",    Style::default().fg(Theme::FAIL)),
        Some(DeployStatus::Cancel)  =>
            Span::styled("○ cancelled", Style::default().fg(Theme::CANCEL)),
        Some(DeployStatus::Running) =>
            Span::styled("◐ running",   Style::default().fg(Theme::INFO)),
        Some(DeployStatus::Pending) =>
            Span::styled("◌ pending",   Style::default().fg(Theme::WARN)),
    }
}

pub fn branch_span(branch: &str) -> Span<'static> {
    if branch.is_empty() || branch == "-" {
        return Span::styled("—", Style::default().fg(Theme::TEXT_GHOST));
    }
    Span::styled(format!("⎇ {}", branch), Style::default().fg(Theme::TEXT_LO))
}
```

### Search mode

When `/` is pressed, render a single-line search bar above the table:

```rust
// In layout, when search is active, add a Length(1) before the table:
let [search_bar, table_area] = Layout::default()
    .direction(Direction::Vertical)
    .constraints([Constraint::Length(1), Constraint::Min(0)])
    .split(content_area)
    .try_into().unwrap();

// Render search bar
let search_line = Line::from(vec![
    Span::styled("/ ", Style::default().fg(Theme::CANCEL)),
    Span::styled(&app.search_query, Style::default().fg(Theme::PASS)),
    Span::styled("█", Style::default().fg(Theme::PASS)), // cursor blink via app state
]);
frame.render_widget(
    Paragraph::new(search_line).style(Style::default().bg(Theme::BG_SURFACE)),
    search_bar,
);
```

---

## 4. Tab 2 — Platform

### Concept

Every raw deployment event from every platform, in one chronological stream. The key addition over the current implementation: **pipeline stages column** for GitHub rows, showing the job sequence inline. Railway rows show the deploy service instead. Vercel shows the environment.

### Pipeline stages — the key design move

```
GitHub row:  ● Restore › ● Build › ✕ Tests › ○ Integ.
Railway row: tasuky                                      (service name)
Vercel row:  Production                                  (environment)
```

```rust
fn pipeline_cell(event: &PlatformEvent) -> Cell<'static> {
    match event.platform {
        Platform::GitHub => {
            let stages = render_gh_pipeline(&event.jobs);
            Cell::from(stages)
        }
        Platform::Railway => {
            Cell::from(Span::styled(
                event.service.clone().unwrap_or_default(),
                Style::default().fg(Theme::RW),
            ))
        }
        Platform::Vercel => {
            Cell::from(Span::styled(
                event.environment.clone().unwrap_or_default(),
                Style::default().fg(Theme::VC),
            ))
        }
    }
}

fn render_gh_pipeline(jobs: &[JobRun]) -> Line<'static> {
    let stage_names = ["Restore", "Build", "Tests", "Integ."];
    let arrow = Span::styled(" › ", Style::default().fg(Theme::TEXT_GHOST));

    let mut spans: Vec<Span> = vec![];
    for (i, job) in jobs.iter().enumerate() {
        if i > 0 { spans.push(arrow.clone()); }
        let (icon, color) = match job.status {
            JobStatus::Success => ("●", Theme::PASS),
            JobStatus::Failure => ("✕", Theme::FAIL),
            JobStatus::Skipped => ("○", Theme::TEXT_GHOST),
            JobStatus::Running => ("◐", Theme::INFO),
        };
        let label = stage_names.get(i).copied().unwrap_or("Job");
        spans.push(Span::styled(
            format!("{} {}", icon, label),
            Style::default().fg(color),
        ));
    }
    Line::from(spans)
}
```

### Column widths

```rust
fn platform_column_widths() -> Vec<Constraint> {
    vec![
        Constraint::Length(12),   // status
        Constraint::Length(8),    // platform
        Constraint::Length(22),   // title
        Constraint::Length(9),    // sha
        Constraint::Min(36),      // pipeline / detail (fills)
        Constraint::Length(12),   // actor
        Constraint::Length(7),    // age
        Constraint::Length(8),    // duration
    ]
}
```

### Platform icon cell

```rust
fn platform_cell(platform: Platform) -> Cell<'static> {
    let (label, color) = match platform {
        Platform::GitHub  => ("GH", Theme::GH),
        Platform::Railway => ("RW", Theme::RW),
        Platform::Vercel  => ("VC", Theme::VC),
    };
    Cell::from(Span::styled(label, Style::default().fg(color).add_modifier(Modifier::BOLD)))
}
```

---

## 5. Tab 3 — Health

### Concept

Split pane. Left: scrollable project list with score and sparkline. Right: detail panel for selected project with weighted breakdown and recent events.

### Layout

```rust
let [list_pane, detail_pane] = Layout::default()
    .direction(Direction::Horizontal)
    .constraints([
        Constraint::Length(34),   // project list
        Constraint::Min(0),       // detail
    ])
    .split(content_area)
    .try_into().unwrap();
```

### Left pane — project list

Each list item is 2 rows tall:

```
 acslogistica-control-cen   0   ▁▁▁▁▁▁▁▁
   Critical
 ─────────────────────────────────────────
 cxnow                    100   ▃▅█▇██▇█
   Healthy
```

```rust
use ratatui::widgets::List;

fn build_health_list(projects: &[ProjectHealth]) -> List<'_> {
    let items: Vec<ListItem> = projects.iter().map(|p| {
        let color = health_color(p.score, &p.status);
        let sparkline = render_sparkline_text(&p.history_8, color);

        // Row 1: name + score + sparkline
        let row1 = Line::from(vec![
            Span::styled(truncate(&p.name, 22), Style::default().fg(Theme::TEXT_HI)),
            Span::raw("  "),
            Span::styled(
                format!("{:>3}", p.score),
                Style::default().fg(color).add_modifier(Modifier::BOLD),
            ),
            Span::raw("  "),
            sparkline,
        ]);
        // Row 2: status label (indented)
        let row2 = Line::from(vec![
            Span::raw("  "),
            Span::styled(
                p.status.label(),
                Style::default().fg(color),
            ),
        ]);
        ListItem::new(vec![row1, row2])
    }).collect();

    List::new(items)
        .highlight_style(Theme::selected_row())
        .highlight_symbol("▶ ")
}

fn health_color(score: u8, status: &HealthStatus) -> Color {
    match status {
        HealthStatus::Healthy  => Theme::PASS,
        HealthStatus::Critical => if score > 60 { Theme::WARN } else { Theme::FAIL },
        HealthStatus::Degraded => Theme::WARN,
    }
}
```

### Sparkline (text-based, using block elements)

Ratatui has a built-in `Sparkline` widget, but it renders all in one color. For per-bar coloring, render manually using block chars:

```rust
fn render_sparkline_text(values: &[u8], color: Color) -> Span<'static> {
    // Map 0..100 to block chars ▁▂▃▄▅▆▇█
    let chars = "▁▂▃▄▅▆▇█";
    let spark: String = values.iter().map(|&v| {
        if v == 0 { ' ' }
        else {
            let idx = ((v as usize) * 7 / 100).min(7);
            chars.chars().nth(idx).unwrap_or('▁')
        }
    }).collect();
    Span::styled(spark, Style::default().fg(color))
}

// Alternative: use ratatui's built-in Sparkline widget in its own Rect
// Sparkline::default()
//     .data(&values_u64)
//     .style(Style::default().fg(color))
//     .max(100)
```

### Right pane — detail panel

Decomposed into stacked sections using vertical layout:

```
  pulsos-cli                                          50
  Health score · weighted average                   /100

  PLATFORM WEIGHTS
  GH  40%  ████████████░░░░░░░░░░░░  50%
  RW  35%  ░░░░░░░░░░░░░░░░░░░░░░░░   0%
  VC  25%  ░░░░░░░░░░░░░░░░░░░░░░░░   0%

  RECENT EVENTS
  ●  1d ago   Foundation — GitHub CI failed         fail
  ●  1d ago   Implement correlation — push failed   fail
  ●  3d ago   Foundation — GitHub CI passed         pass
  ●  3d ago   Add basic CI — push passed            pass

  HISTORY (LAST 8)
  ██  ██  ░░  ░░  ██  ██  ░░  ▄▄
```

```rust
fn render_health_detail(frame: &mut Frame, area: Rect, project: &ProjectHealth) {
    // Outer block with title
    let block = Block::default()
        .borders(Borders::LEFT)
        .border_style(Style::default().fg(Theme::BORDER));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let [header, weights_section, events_section, history_section] =
        Layout::default()
            .direction(Direction::Vertical)
            .margin(1)
            .constraints([
                Constraint::Length(3),    // name + score
                Constraint::Length(6),    // weights
                Constraint::Min(0),       // events (fills)
                Constraint::Length(4),    // history bars
            ])
            .split(inner)
            .try_into().unwrap();

    render_detail_header(frame, header, project);
    render_weight_bars(frame, weights_section, project);
    render_events_list(frame, events_section, project);
    render_history_bars(frame, history_section, project);
}
```

**Weight bars:**

```rust
fn render_weight_bars(frame: &mut Frame, area: Rect, project: &ProjectHealth) {
    let weights = [
        ("GH", Theme::GH,  40, project.weights.github),
        ("RW", Theme::RW,  35, project.weights.railway),
        ("VC", Theme::VC,  25, project.weights.vercel),
    ];

    // Section title
    let title = Paragraph::new("PLATFORM WEIGHTS")
        .style(Style::default().fg(Theme::TEXT_GHOST).add_modifier(Modifier::BOLD));

    // Each bar row: "GH  40%  ████████████░░░░  score%"
    let bar_width = (area.width as usize).saturating_sub(24);

    let lines: Vec<Line> = weights.iter().map(|(label, color, weight_pct, score)| {
        let filled = (bar_width * (*score as usize) / 100).min(bar_width);
        let empty = bar_width - filled;
        let bar_color = if *score == 0 { Theme::FAIL } else { *color };

        Line::from(vec![
            Span::styled(format!("{}", label),
                Style::default().fg(*color).add_modifier(Modifier::BOLD)),
            Span::styled(format!(" {:>3}%  ", weight_pct),
                Style::default().fg(Theme::TEXT_GHOST)),
            Span::styled("█".repeat(filled), Style::default().fg(bar_color)),
            Span::styled("░".repeat(empty),  Style::default().fg(Theme::BG_RAISED)),
            Span::styled(format!("  {:>3}%", score),
                Style::default().fg(bar_color)),
        ])
    }).collect();

    frame.render_widget(Paragraph::new(lines), area);
}
```

---

## 6. Tab 4 — Settings

### Concept

Split pane: left = platform list with state badges, right = structured key-value detail + action keybinding grid.

### Layout

```rust
let [list_pane, detail_pane] = Layout::default()
    .direction(Direction::Horizontal)
    .constraints([
        Constraint::Length(24),   // platform list
        Constraint::Min(0),       // detail
    ])
    .split(content_area)
    .try_into().unwrap();
```

### Left pane — platform list

```
 GitHub    ! Needs Config
 Railway   ✓ Ready
 Vercel    ~ Conn. Err
```

```rust
fn build_settings_list(platforms: &[PlatformSettings]) -> List<'_> {
    let items: Vec<ListItem> = platforms.iter().map(|p| {
        let (symbol, color) = match p.state {
            PlatformState::Ok          => ("✓ Ready",        Theme::PASS),
            PlatformState::NeedsConfig => ("! Needs Config", Theme::WARN),
            PlatformState::ConnError   => ("~ Conn. Err",    Theme::FAIL),
        };
        let line = Line::from(vec![
            Span::styled(format!("{:<10}", p.name),
                Style::default().fg(Theme::TEXT_HI).add_modifier(Modifier::BOLD)),
            Span::styled(symbol, Style::default().fg(color).add_modifier(Modifier::BOLD)),
        ]);
        ListItem::new(line)
    }).collect();

    List::new(items)
        .highlight_style(Theme::selected_row())
}
```

### Right pane — detail

Key-value table + actions grid:

```
  GitHub — Auth & Onboarding
  ─────────────────────────────────────────────
  Provider     GitHub (! Needs Config)
  Token source keyring
  Tracked      9 repos
  Reason       9 repo(s) configured but not accessible

  ACTIONS
  [t] Set token    [T] Override    [v] Validate    [x] Remove
  [o] Onboard      [↵] Providers

  ! Check repo/org access and token scopes, then run
    `pulsos repos verify`
```

```rust
fn render_settings_detail(frame: &mut Frame, area: Rect, platform: &PlatformSettings) {
    let block = Block::default()
        .borders(Borders::LEFT)
        .border_style(Style::default().fg(Theme::BORDER));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let [header, kv_section, actions_section, next_action] =
        Layout::default()
            .direction(Direction::Vertical)
            .margin(1)
            .constraints([
                Constraint::Length(2),    // platform name + subtitle
                Constraint::Length(5),    // key-value pairs
                Constraint::Length(4),    // action keybindings
                Constraint::Min(0),       // next action callout
            ])
            .split(inner)
            .try_into().unwrap();

    render_settings_kv(frame, kv_section, platform);
    render_action_keys(frame, actions_section);
    render_next_action(frame, next_action, platform);
}

fn render_settings_kv(frame: &mut Frame, area: Rect, p: &PlatformSettings) {
    let state_color = match p.state {
        PlatformState::Ok          => Theme::PASS,
        PlatformState::NeedsConfig => Theme::WARN,
        PlatformState::ConnError   => Theme::FAIL,
    };

    let rows = vec![
        kv_row("Provider",     &format!("{} — {}", p.name, p.state.label()), state_color),
        kv_row("Token source", &p.token_source, Theme::TEXT_MID),
        kv_row("Tracked",      &format!("{} repos", p.tracked_count), Theme::TEXT_MID),
        kv_row("Reason",       &p.reason, Theme::TEXT_LO),
    ];

    frame.render_widget(Paragraph::new(rows), area);
}

fn kv_row(key: &str, val: &str, val_color: Color) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!("{:<14}", key),
            Style::default().fg(Theme::TEXT_GHOST).add_modifier(Modifier::BOLD)),
        Span::styled(val.to_owned(), Style::default().fg(val_color)),
    ])
}

fn render_action_keys(frame: &mut Frame, area: Rect) {
    let actions = vec![
        ("[t]", "Set token"), ("[T]", "Override"),
        ("[v]", "Validate"),  ("[x]", "Remove"),
        ("[o]", "Onboard"),   ("[↵]", "Providers"),
    ];

    let title = Line::from(Span::styled("ACTIONS",
        Style::default().fg(Theme::TEXT_GHOST).add_modifier(Modifier::BOLD)));

    let row1 = action_key_line(&actions[..4]);
    let row2 = action_key_line(&actions[4..]);

    frame.render_widget(Paragraph::new(vec![title, row1, row2]), area);
}

fn action_key_line(actions: &[(&str, &str)]) -> Line<'static> {
    let mut spans = vec![];
    for (key, label) in actions {
        spans.push(Span::styled(key.to_string(),
            Style::default().fg(Theme::TEXT_HI).add_modifier(Modifier::BOLD)));
        spans.push(Span::raw(" "));
        spans.push(Span::styled(format!("{:<14}", label),
            Style::default().fg(Theme::TEXT_LO)));
    }
    Line::from(spans)
}

fn render_next_action(frame: &mut Frame, area: Rect, p: &PlatformSettings) {
    let (icon, color) = match p.state {
        PlatformState::Ok => ("●", Theme::PASS),
        _                 => ("!", Theme::WARN),
    };
    let text = vec![
        Line::from(vec![
            Span::styled(format!("{} ", icon), Style::default().fg(color)),
            Span::styled(&p.next_action, Style::default().fg(color)),
        ]),
    ];
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(color));
    frame.render_widget(
        Paragraph::new(text).block(block).wrap(Wrap { trim: false }),
        area,
    );
}
```

---

## 7. Tab 5 — Logs

### Concept

Chronological stream of log entries. Filter bar at the top. Each row: timestamp, level badge, repo path, message, optional error snippet.

### Layout

```rust
let [filter_bar, log_stream] = Layout::default()
    .direction(Direction::Vertical)
    .constraints([Constraint::Length(1), Constraint::Min(0)])
    .split(content_area)
    .try_into().unwrap();
```

### Filter bar

```
 [WARN] [ERR] [INFO] [ALL]                               14 entries
```

```rust
fn render_log_filter_bar(frame: &mut Frame, area: Rect, active: LogLevel) {
    let filters = [LogLevel::Warn, LogLevel::Err, LogLevel::Info, LogLevel::All];
    let mut spans = vec![];
    for level in filters {
        let is_active = level == active;
        let label = format!("[{}]", level.label());
        let style = if is_active {
            Style::default().fg(Theme::WARN).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Theme::TEXT_GHOST)
        };
        spans.push(Span::styled(label, style));
        spans.push(Span::raw(" "));
    }
    frame.render_widget(
        Paragraph::new(Line::from(spans))
            .style(Style::default().bg(Theme::BG_SURFACE)),
        area,
    );
}
```

### Log rows

Each row is 1 line:

```
 14:23:26  WARN  Vivallo04/acslogistica-control-center  Failed to fetch GitHub runs, trying cache
 14:24:40   ERR  vercel                                 API request failed: ECONNREFUSED
```

```rust
fn build_log_rows(entries: &[LogEntry], filter: LogLevel) -> Vec<Row<'_>> {
    entries.iter()
        .filter(|e| filter == LogLevel::All || e.level == filter)
        .map(|e| {
            let (level_str, level_color) = match e.level {
                LogLevel::Warn => ("WARN", Theme::WARN),
                LogLevel::Err  => ("ERR",  Theme::FAIL),
                LogLevel::Info => ("INFO", Theme::INFO),
                LogLevel::All  => unreachable!(),
            };
            Row::new(vec![
                Cell::from(Span::styled(&e.timestamp, Theme::muted())),
                Cell::from(Span::styled(level_str,
                    Style::default().fg(level_color).add_modifier(Modifier::BOLD))),
                Cell::from(Span::styled(
                    format!("Vivallo04/{}", &e.repo),
                    Style::default().fg(Theme::INFO),
                )),
                Cell::from(Span::styled(&e.message, Theme::muted())),
                Cell::from(Span::styled(
                    e.error.as_deref().unwrap_or(""),
                    Style::default().fg(Theme::FAIL),
                )),
            ])
        })
        .collect()
}

fn log_column_widths() -> Vec<Constraint> {
    vec![
        Constraint::Length(10),   // timestamp
        Constraint::Length(5),    // level
        Constraint::Length(38),   // repo
        Constraint::Min(0),       // message
        Constraint::Length(22),   // error snippet
    ]
}
```

---

## 8. Selection & Keyboard Wiring

### App state

```rust
pub struct App {
    pub active_tab: Tab,
    pub unified_state:  TableState,
    pub platform_state: TableState,
    pub health_state:   ListState,
    pub settings_state: ListState,
    pub log_filter:     LogLevel,
    pub search_active:  bool,
    pub search_query:   String,
}

#[derive(Clone, Copy, PartialEq)]
pub enum Tab { Unified, Platform, Health, Settings, Logs }
```

### Key handler

```rust
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

pub fn handle_key(app: &mut App, key: KeyEvent) {
    // Tab switching — number keys
    match key.code {
        KeyCode::Char('1') => app.active_tab = Tab::Unified,
        KeyCode::Char('2') => app.active_tab = Tab::Platform,
        KeyCode::Char('3') => app.active_tab = Tab::Health,
        KeyCode::Char('4') => app.active_tab = Tab::Settings,
        KeyCode::Char('5') => app.active_tab = Tab::Logs,
        KeyCode::Tab => app.cycle_tab(false),
        KeyCode::BackTab => app.cycle_tab(true),
        _ => {}
    }

    // Search mode
    if app.search_active {
        match key.code {
            KeyCode::Esc   => { app.search_active = false; app.search_query.clear(); }
            KeyCode::Enter => { app.search_active = false; }
            KeyCode::Char(c) => app.search_query.push(c),
            KeyCode::Backspace => { app.search_query.pop(); }
            _ => {}
        }
        return;
    }

    // Tab-specific keys
    match app.active_tab {
        Tab::Unified => match key.code {
            KeyCode::Char('/') => app.search_active = true,
            KeyCode::Char('j') | KeyCode::Down => app.unified_state.select_next(),
            KeyCode::Char('k') | KeyCode::Up   => app.unified_state.select_prev(),
            _ => {}
        },
        Tab::Platform => match key.code {
            KeyCode::Char('j') | KeyCode::Down => app.platform_state.select_next(),
            KeyCode::Char('k') | KeyCode::Up   => app.platform_state.select_prev(),
            _ => {}
        },
        Tab::Health => match key.code {
            KeyCode::Char('j') | KeyCode::Down => app.health_state.select_next(),
            KeyCode::Char('k') | KeyCode::Up   => app.health_state.select_prev(),
            _ => {}
        },
        Tab::Settings => match key.code {
            KeyCode::Char('j') | KeyCode::Down => app.settings_state.select_next(),
            KeyCode::Char('k') | KeyCode::Up   => app.settings_state.select_prev(),
            KeyCode::Char('t') => { /* trigger token input flow */ }
            KeyCode::Char('v') => { /* trigger validate */ }
            KeyCode::Char('x') => { /* trigger remove */ }
            KeyCode::Char('o') | KeyCode::Enter => { /* trigger onboard */ }
            _ => {}
        },
        Tab::Logs => match key.code {
            KeyCode::Char('f') => app.log_filter = app.log_filter.next(),
            KeyCode::Char('j') | KeyCode::Down => { /* scroll log */ }
            KeyCode::Char('k') | KeyCode::Up   => { /* scroll log */ }
            _ => {}
        },
    }

    // Global
    match key.code {
        KeyCode::Char('r') => { /* trigger refresh */ }
        KeyCode::Char('q') | KeyCode::Char('c')
            if key.modifiers.contains(KeyModifiers::CONTROL)
            => { /* quit */ }
        _ => {}
    }
}
```

---

## 9. Render Loop Structure

```rust
// src/tui/mod.rs
pub fn run(terminal: &mut Terminal<impl Backend>, app: &mut App) -> Result<()> {
    loop {
        terminal.draw(|frame| render_frame(frame, app))?;

        if event::poll(Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                handle_key(app, key);
                if app.should_quit { break; }
            }
        }

        // Background refresh tick
        if app.refresh_due() {
            app.trigger_refresh();
        }
    }
    Ok(())
}

fn render_frame(frame: &mut Frame, app: &App) {
    let [titlebar, content, statusbar] = root_layout(frame.area());

    render_titlebar(frame, titlebar, app);
    render_statusbar(frame, statusbar, app);

    match app.active_tab {
        Tab::Unified  => render_unified(frame, content, app),
        Tab::Platform => render_platform(frame, content, app),
        Tab::Health   => render_health(frame, content, app),
        Tab::Settings => render_settings(frame, content, app),
        Tab::Logs     => render_logs(frame, content, app),
    }
}
```

---

## 10. File Structure

```
crates/pulsos-cli/src/tui/
├── mod.rs              # run() loop, Terminal setup/teardown
├── app.rs              # App state struct, tab enum, action dispatch
├── keys.rs             # handle_key()
├── layout.rs           # root_layout(), titlebar_layout() — pure Rect math
├── theme.rs            # Theme constants, style helpers
├── render/
│   ├── mod.rs          # render_frame() dispatcher
│   ├── titlebar.rs     # wordmark, tabs, platform pills
│   ├── statusbar.rs    # shortcuts, warn pill
│   ├── unified.rs      # tab 1
│   ├── platform.rs     # tab 2
│   ├── health.rs       # tab 3 — list + detail
│   ├── settings.rs     # tab 4 — list + detail
│   └── logs.rs         # tab 5
└── widgets/
    ├── status.rs       # status_badge(), branch_span()
    ├── sparkline.rs    # render_sparkline_text()
    ├── weight_bar.rs   # render_weight_bars()
    └── pipeline.rs     # render_gh_pipeline()
```

---

## 11. Quick Reference — Design Decisions

| Decision | Implementation |
|---|---|
| Active tab indicator | `Tabs::highlight_style` with `.underline_color(PASS)` |
| Status icons | `●` pass, `✕` fail, `○` cancel — never colored X or checkmark chars |
| Selected row | `bg(BG_RAISED)` — subtle lift, not an accent color |
| Platform colors | GH near-white, RW violet, VC sky — consistent throughout all tabs |
| Sparklines | Manual block-char rendering for color control, OR ratatui `Sparkline` |
| Score display | Number only — no ring, no gauge. Score rings are not terminal-appropriate |
| Weight bars | `█` filled + `░` empty — same pattern as ratatui `Gauge` but inline in Paragraph |
| Split panes | Health + Settings use `Length(34)` / `Length(24)` left + `Min(0)` right |
| Search bar | Injected `Length(1)` row above table when active, not a popup |
| Token input | Use existing settings flow — crossterm raw input line in a `Paragraph` block |
| Log coloring | Level badge colored, repo path in INFO blue, error snippet in FAIL red |
| Wordmark | `P` in PASS green, `ULSOS` in TEXT_HI bold — spaced letters are visual, not literal spaces |
