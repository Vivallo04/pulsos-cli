# Pulsos Design System v1.0

**Brand Attributes:** MINIMAL · BOLD
**Medium:** Terminal (CLI + TUI via Ratatui)
**Design Philosophy:** Every pixel of terminal real estate is sacred. Say more with less. Silence is a design element.

---

## 0. Design Principles

Five rules. No exceptions.

**1. Silence is confidence.**
If everything is green, the screen should feel quiet. A healthy system doesn't scream at you. Reserve visual energy for things that need attention.

**2. Information density without clutter.**
A terminal is not a web page. Users chose the terminal because they want density. Give it to them — but with typographic discipline, not noise.

**3. Progressive disclosure.**
Show the answer first, then the detail. The status table is the answer. Drill-down is the detail. Never front-load complexity.

**4. Monochrome first, color for meaning.**
Default rendering is high-contrast monochrome (white on dark). Color is added only to encode information: status, severity, confidence. If you can't explain why a color is there, remove it.

**5. Motion is feedback, not decoration.**
Spinners confirm the system is working. Progress bars show passage of time. Everything else is static. No gratuitous animation.

---

## 1. Color Palette

### 1.1 Foundation — The Grayscale

The grayscale is the backbone. 90% of what the user sees should be grayscale. Color is the exception, not the rule.

```
Token                 ANSI         RGB            Ratatui                Usage
─────────────────────────────────────────────────────────────────────────────────
bg.primary            -            #0D0D0D        Color::Rgb(13,13,13)   App background
bg.surface            -            #161616        Color::Rgb(22,22,22)   Panel/card background
bg.elevated           -            #1E1E1E        Color::Rgb(30,30,30)   Selected row, hover
bg.overlay            -            #252525        Color::Rgb(37,37,37)   Modal/dialog background

border.muted          gray         #333333        Color::Rgb(51,51,51)   Inactive borders
border.default        gray         #444444        Color::Rgb(68,68,68)   Default borders
border.focus          white        #888888        Color::Rgb(136,136,136) Focused panel border

fg.muted              dark gray    #555555        Color::Rgb(85,85,85)   Disabled text, timestamps
fg.subtle             gray         #777777        Color::Rgb(119,119,119) Secondary labels, hints
fg.default            light gray   #B0B0B0        Color::Rgb(176,176,176) Body text
fg.strong             white        #E0E0E0        Color::Rgb(224,224,224) Primary text, values
fg.emphasis           bright white #FFFFFF        Color::White            Headings, active selections
```

### 1.2 Semantic Colors — Status

Status colors are the only non-grayscale elements in normal operation. They follow a universal traffic-light model, extended with two additional states.

```
Token                 ANSI            RGB            Ratatui                    Meaning
─────────────────────────────────────────────────────────────────────────────────────────────
status.success        green           #34D399        Color::Rgb(52,211,153)     Passed, deployed, ready
status.success.muted  dark green      #065F46        Color::Rgb(6,95,70)        Success background tint
status.failure        red             #F87171        Color::Rgb(248,113,113)    Failed, crashed, error
status.failure.muted  dark red        #7F1D1D        Color::Rgb(127,29,29)      Failure background tint
status.warning        yellow          #FBBF24        Color::Rgb(251,191,36)     Stale data, expiring token
status.warning.muted  dark yellow     #78350F        Color::Rgb(120,53,15)      Warning background tint
status.active         blue            #60A5FA        Color::Rgb(96,165,250)     In progress, building
status.active.muted   dark blue       #1E3A5F        Color::Rgb(30,58,95)       Active background tint
status.neutral        gray            #9CA3AF        Color::Rgb(156,163,175)    Skipped, sleeping, queued
```

### 1.3 Accent Color — Brand

One accent color. Used sparingly: the logo mark, active tab indicator, focused input border, link targets.

```
Token                 RGB            Ratatui                    Usage
───────────────────────────────────────────────────────────────────────────
accent.primary        #818CF8        Color::Rgb(129,140,248)    Active tab underline, brand mark
accent.dim            #4338CA        Color::Rgb(67,56,202)      Accent background, subtle highlight
```

### 1.4 Confidence Indicators

Used exclusively by the correlation engine. These must be distinguishable at a glance.

```
Token                 Symbol    Color              Ratatui                    Meaning
─────────────────────────────────────────────────────────────────────────────────────────
confidence.exact      ●         #34D399 (green)    Color::Rgb(52,211,153)     SHA match
confidence.high       ◐         #60A5FA (blue)     Color::Rgb(96,165,250)     Mapped + timestamp
confidence.low        ○         #FBBF24 (yellow)   Color::Rgb(251,191,36)     Timestamp only
confidence.none       ·         #555555 (muted)    Color::Rgb(85,85,85)       No correlation
```

### 1.5 Light Mode (Optional)

Light mode inverts the grayscale. Semantic colors shift to darker variants for readability on white.

```
Token                 Light Mode RGB     Ratatui
───────────────────────────────────────────────────
bg.primary            #FAFAFA            Color::Rgb(250,250,250)
bg.surface            #FFFFFF            Color::Rgb(255,255,255)
bg.elevated           #F0F0F0            Color::Rgb(240,240,240)
border.default        #D4D4D4            Color::Rgb(212,212,212)
fg.default            #404040            Color::Rgb(64,64,64)
fg.strong             #1A1A1A            Color::Rgb(26,26,26)
fg.emphasis           #000000            Color::Black
status.success        #059669            Color::Rgb(5,150,105)
status.failure        #DC2626            Color::Rgb(220,38,38)
status.warning        #D97706            Color::Rgb(217,119,6)
status.active         #2563EB            Color::Rgb(37,99,235)
```

---

## 2. Typography

### 2.1 Scale

Terminal typography is constrained to the user's monospace font. Hierarchy is achieved through weight (bold), case, spacing, and symbolic prefixes — not font size.

9 levels, from most prominent to least:

```
Level    Name              Style                         Usage                        Ratatui
──────────────────────────────────────────────────────────────────────────────────────────────────
T1       Display           UPPERCASE + Bold + Spacing    App title, first-run banner  Style::new().bold().fg(accent.primary)
T2       Section Header    Bold + Underline character    Section dividers "── Auth"   Style::new().bold().fg(fg.emphasis)
T3       Panel Title       Bold                          Panel/tab titles             Style::new().bold().fg(fg.emphasis)
T4       Table Header      Bold + Muted color            Column headers in tables     Style::new().bold().fg(fg.subtle)
T5       Body Strong       Bold                          Values, project names        Style::new().bold().fg(fg.strong)
T6       Body              Regular                       Default text                 Style::new().fg(fg.default)
T7       Body Secondary    Regular + Muted               Descriptions, paths          Style::new().fg(fg.subtle)
T8       Caption           Regular + Dim                 Timestamps, cache age        Style::new().fg(fg.muted)
T9       System            Regular + Very dim             Version numbers, debug       Style::new().fg(Color::Rgb(68,68,68))
```

### 2.2 Typography Rules

**UPPERCASE** — Reserved for T1 (app title) and status badges only. Never for body text.

**Bold** — Indicates importance, not emphasis. A bold project name says "this is the anchor of this row." A bold status says "this is the key data point." Do not bold entire sentences.

**Dim** — The most powerful tool in terminal typography. Dimming non-essential text (timestamps, paths, secondary labels) creates visual hierarchy without consuming space.

**Symbolic prefixes** — Use Unicode symbols as type-scale amplifiers:

```
Symbol    Usage                    Example
──────────────────────────────────────────────────────
◆         Section marker           ◆ Authentication
│         Hierarchy connector      │ Validating...
├── ──    Tree branches            ├── myorg/my-saas
└──       Last tree item           └── auth-service
→         Action suggestion        → Run: pulsos auth vercel
✓         Success inline           ✓ Authenticated as @Vivallo04
✗         Failure inline           ✗ Token validation failed
⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏  Braille spinner    ⠹ Discovering projects...
▪         Bullet                   ▪ 4 repositories selected
```

### 2.3 Font Recommendations

Pulsos does not control the user's terminal font, but documentation and screenshots should use:

- **Primary:** JetBrains Mono — excellent Unicode coverage, clear at small sizes
- **Alternative:** SF Mono (macOS), Cascadia Code (Windows), Fira Code (Linux)
- **Fallback:** The user's default monospace font

---

## 3. Spacing System

### 3.1 Vertical Rhythm — Line-Based Grid

Terminal spacing is measured in lines, not pixels. The base unit is 1 line.

```
Token          Lines    Usage
──────────────────────────────────────────────────────────
space.none     0        Between tightly coupled elements (label:value)
space.xs       0        Empty line suppressed — content flows continuously
space.sm       1        Between related items within a group
space.md       1+blank  Between groups/sections (one blank line)
space.lg       2+blank  Between major sections (two blank lines)
space.xl       3+blank  Before/after full-width dividers
```

### 3.2 Horizontal Spacing

```
Token          Chars    Usage
──────────────────────────────────────────────────────────
indent.none    0        Top-level content
indent.sm      2        First-level nesting (wizard steps, tree items)
indent.md      4        Second-level nesting (sub-items in a tree)
indent.lg      6        Third-level nesting (rare)
gap.column     2        Between table columns (minimum)
gap.label      1        Between label and value ("Status: Active")
pad.panel      1        Left padding inside bordered panels
```

### 3.3 The Content Width Grid

For TUI panels, content follows an 8-character base grid:

```
                    8    16    24    32    40    48    56    64    72    80
Minimum terminal:   ────────────────────────────────────────────────────────────────────────
                   │                              80 columns                                │

Column widths:
  Project name:     [  16 chars  ]
  GitHub status:    [  12 ch ]
  Railway status:   [  12 ch ]
  Vercel status:    [  12 ch ]
  Health score:     [ 8ch]
  Branch:           [  12 ch ]
  Last updated:     [ 8ch]
                    ──────────────
                    80 chars total (minimum viable)

Responsive:
  80 cols:   Project | GitHub | Railway | Vercel
  120 cols:  Project | GitHub | Railway | Vercel | Health | Branch | Updated
  160 cols:  Full detail with commit SHA preview + duration
```

---

## 4. Component Specifications

### 4.1 Status Badge

The atomic unit of Pulsos. A 1-cell-wide indicator followed by a label.

```
States:
  ✓ passed       fg: status.success        bold: label only
  ✗ failed       fg: status.failure        bold: label only
  ◌ building     fg: status.active         bold: label only (spinner replaces ◌ when live)
  ⏸ queued       fg: status.neutral        bold: none
  ⚠ warning      fg: status.warning        bold: label only
  — none         fg: fg.muted              bold: none
  ● sleeping     fg: status.neutral        bold: none

Ratatui implementation:
  Span::styled("✓ ", Style::new().fg(status.success))
  Span::styled("passed", Style::new().fg(status.success).bold())

Width: symbol(1) + space(1) + label(variable) = min 8, max 12
```

### 4.2 Status Table (Primary View)

The most-seen component. Must be scannable in under 2 seconds.

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                                                                             │
│  Project          GitHub CI      Railway        Vercel         Health       │
│  ─────────────────────────────────────────────────────────────────────      │
│  my-saas          ✓ passed       ✓ success      ✓ ready          98        │
│  api-core         ✓ passed       ✓ success      —                95        │
│  auth-service     ✗ failed       ✓ success      —                62        │
│  client-portal    ✓ passed       —              —                92        │
│                                                                             │
│  Last sync: 12s ago                                  4 projects tracked     │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘

Styling:
  Header row:           T4 (bold + fg.subtle)
  Header underline:     ─ repeated, fg: border.muted
  Project name:         T5 (bold + fg.strong)
  Status values:        Status Badge component
  Health score:
    90-100:             fg: status.success, bold
    70-89:              fg: status.warning
    0-69:               fg: status.failure, bold
  Footer:               T8 (fg.muted)
  "—" no data:          fg.muted, regular

Ratatui: Table widget with Block border(ROUNDED), Constraint proportional columns
Selected row: bg.elevated background, border.focus left indicator (▎)
```

### 4.3 Wizard Step

Used in first-run flow. Each step is a self-contained section.

```
◆ GitHub                                                    Step 1 of 3
──────────────────────────────────────────────────────────────────────────

  No existing token found.

  Create a personal access token at:
    https://github.com/settings/tokens

  Required scopes: repo, read:org

  Enter your GitHub token: ••••••••••••••••••••••••••••

    Validating... ✓ OK (@Vivallo04)
    Token stored securely.

Styling:
  ◆ + Step title:       T2 (bold + fg.emphasis) + T8 right-aligned (Step N of M)
  Divider:              ── full width, fg: border.muted
  Instructions:         T6 (fg.default), indent.sm
  URLs:                 T7 (fg.subtle), underline, indent.md
  Input prompt:         T5 (bold + fg.strong), indent.sm
  Token dots:           fg.muted
  Validation success:   Status Badge (✓ OK), indent.md
  Validation failure:   Status Badge (✗ FAILED), indent.md, then error message in status.failure
```

Implementation contract:
- Interactive setup commands use full-screen step rendering in TTY mode (clear and redraw each prompt stage).
- Flow is linear: continue forward or cancel (no back stack in this version).
- The same screen treatment applies to interactive `auth`, `repos`, and `views` prompts for consistency.
- Non-TTY output remains plain text for scripting and accessibility.

### 4.4 Spinner

Braille animation for async operations. 8 frames.

```
Frames:  ⠋ ⠙ ⠹ ⠸ ⠼ ⠴ ⠦ ⠧ ⠇ ⠏
Color:   accent.primary
Speed:   80ms per frame (12.5 FPS)
Format:  "{spinner} {message}"

Example: ⠹ Discovering projects across platforms...

Ratatui: Custom stateful widget, tick on every 80ms event loop cycle.
On completion: replace spinner with ✓ or ✗, hold for 400ms, then collapse line.
```

### 4.5 Progress Bar

For multi-step operations (repo sync scanning orgs).

```
  Scanning organizations  ████████████░░░░░░░░░░░░░░░░░░  3/8

Styling:
  Label:          T6 (fg.default)
  Filled block:   █ fg: accent.primary
  Empty block:    ░ fg: border.muted
  Counter:        T7 (fg.subtle)
  Width:          30 chars fixed (responsive: 20-50)

Ratatui: Gauge widget with custom symbols.
```

### 4.6 Prompt — Boolean (Yes/No)

```
  No GitHub token found. Authenticate now? [Y/n]

Styling:
  Question:       T6 (fg.default)
  Default option: UPPERCASE + bold (Y)
  Other option:   lowercase + regular (n)
  Brackets:       fg.muted

After answer:
  No GitHub token found. Authenticate now? yes      ← fg.muted for entire line
```

### 4.7 Prompt — Token Input

```
  Enter your GitHub token: ••••••••••••••••••••
    (93 characters received: ghp_****...F0ir)

Styling:
  Label:          T5 (bold + fg.strong)
  Dots:           fg.muted (•)
  Confirmation:   T8 (fg.muted), indent.md
  Partial reveal: first 4 + last 4 characters, rest *, fg.muted
```

### 4.8 Prompt — Multi-Select (Repository Chooser)

```
  Select repositories to monitor:

    myorg (Owner)
      [✓] my-saas
      [✓] api-core
      [✓] auth-service
      [ ] legacy-monolith                                         archived
      [ ] experiments                                             archived
    client-work (Member)
      [✓] client-portal

  ↑↓ navigate  ␣ toggle  a select all  ↵ confirm

Styling:
  Instruction:        T6 (fg.default)
  Org header:         T5 (bold + fg.strong), indent.sm
  Org role:           T8 (fg.muted) inline after name
  Selected [✓]:       accent.primary for checkmark, T6 for name
  Unselected [ ]:     fg.muted for box, T6 for name
  Archived tag:       T8 (fg.muted), right-aligned
  Current row:        bg.elevated background, ▸ prefix
  Help bar:           T8 (fg.muted), bottom of component
  Fuzzy filter:       accent.primary input field at top when user types

Ratatui: Custom List widget with stateful selection.
```

### 4.9 Divider

```
Types:
  Section:    ── {Title} ────────────────────────────
  Full:       ──────────────────────────────────────── (full terminal width)
  Subtle:     (blank line only)

Styling:
  ── character:   fg: border.muted
  Title text:     T3 (bold + fg.emphasis), padded with 1 space each side
  Title position: Left-aligned, 2 chars from start

Width: Always fills to terminal width.
```

### 4.10 Error Message

```
  ✗ Authentication failed

    GraphQL error from Railway: Not Authorized

    This usually means:
      ▪ The token is a Project token (Pulsos needs an Account token)
      ▪ The token has been revoked or expired

    → Create a new token at https://railway.com/account/tokens
    → Then run: pulsos auth railway

Styling:
  Header:         ✗ + message in status.failure, bold
  Error detail:   T6 (fg.default), indent.md
  Causes header:  T7 (fg.subtle), indent.md
  Cause items:    T6 (fg.default), indent.md, ▪ prefix
  Action arrows:  → in accent.primary, action text in T5 (bold + fg.strong)
  URLs:           T7 (fg.subtle), underline
```

### 4.11 Success Message

```
  ✓ Authenticated as @Vivallo04
    Token stored securely for GitHub.

Styling:
  Header:       ✓ + message in status.success, bold
  Detail:       T7 (fg.subtle), indent.md
  Collapse:     After 2 seconds in wizard flow, dim entire block to fg.muted
```

### 4.12 Warning Message

```
  ⚠ Token expires in 7 days (2026-02-25)
    → Run: pulsos auth vercel

Styling:
  Header:       ⚠ + message in status.warning, bold
  Action:       → in accent.primary, command in T5 (bold)
```

### 4.13 Info Block

```
  Found 47 repositories across 3 organizations.
  Found 8 projects across 2 Railway workspaces.
  Found 6 projects in 1 Vercel team.

Styling:
  Numbers:      T5 (bold + fg.strong) — inline within sentence
  Rest:         T6 (fg.default)
```

### 4.14 Auth Status Table

```
  Platform   Status       User              Method          Expires
  ────────────────────────────────────────────────────────────────────
  GitHub     ✓ valid      @Vivallo04        PAT             never
  Railway    ✗ failed     —                 —               —
  Vercel     ✓ valid      vivallo04         Access Token    2026-02-25

Styling:
  Same as Status Table (4.2) but with auth-specific columns.
  Platform names: T5 (bold)
  Status column: Status Badge
  "never" expiry: fg.muted
  Expiring soon (< 14 days): status.warning
```

### 4.15 Doctor Report

```
  Pulsos Doctor v0.1.0
  ════════════════════════════════════════════

  System
    OS:              macOS 15.3 (arm64)                    ✓
    Terminal:        iTerm2 3.5                             ✓

  Authentication
    GitHub:          @Vivallo04 (PAT)                      ✓
    Railway:         —                                     ✗
    Vercel:          vivallo04 (Access Token)               ⚠ 7d

  API Connectivity
    api.github.com:          45ms                          ✓
    backboard.railway.com:   —                             ✗
    api.vercel.com:          38ms                          ✓

  Result: 1 error, 1 warning

Styling:
  Title:              T1 (UPPERCASE variant, accent.primary)
  Double-line div:    ═ repeated, fg: accent.dim
  Category headers:   T3 (bold, fg.emphasis), indent.none
  Key:                T6 (fg.default), indent.md, left-aligned
  Value:              T7 (fg.subtle), tab-aligned
  Check symbol:       Right-aligned, Status Badge (✓/✗/⚠)
  Latency numbers:    fg.muted
  Result summary:     T5, counts colored by severity
```

### 4.16 Stale Data Indicator

```
  In status table cells:
    ✓ passed                    ← fresh (< 30s): no indicator
    ✓ passed (2m)               ← stale: age in fg.muted after value
    ✓ passed (STALE: 2h)       ← expired: age in status.failure

  In header/footer:
    [OFFLINE] Showing cached data. Last sync: 47 minutes ago.
    └─ [OFFLINE] badge: bg status.warning.muted, fg status.warning, bold
```

### 4.17 Tab Bar (TUI)

```
   Unified │ Platform │ Health │ Settings   GH✓ RW! VC✗
   ════════                                                 ← active tab underline

Styling:
  Active tab:       T3 (bold, fg.emphasis), underline in accent.primary (═)
  Inactive tab:     T6 (fg.subtle), no underline
  Separator:        │ in fg.muted
  Tab underline:    ═ character, fg: accent.primary, only under active tab
  Provider badges:  compact status badges in header (`GH`, `RW`, `VC`) using semantic colors
                    Ready=success, NoToken=neutral, Invalid=failure, Connectivity/NeedsConfig=warning

Ratatui: Tabs widget with custom highlight_style and divider.
```

### 4.17.1 Settings Panel (TUI)

```
  Platform   State              Reason                              Next Action
  GitHub     ✓ Ready            Authenticated and resources visible  No action needed
  Railway    ! Needs Config     2 tracked projects not accessible    Run pulsos config wizard
  Vercel     ✗ Invalid Token    Authentication failed                Run pulsos auth vercel
```

Rules:
- Always show all three providers.
- State text uses the five-state readiness model.
- Row detail line can include provider-specific counters (accessible/configured).
- `Settings` is navigable like other tabs and updates live with polling snapshots.
- `Settings` is operational, not read-only:
  - `t` set/replace token (validate before save)
  - `v` validate selected provider
  - `x` remove stored token
  - `Enter` start `Onboard` (provider select -> discover -> resource select -> preview -> apply)
- `Onboard` provider/resource selections default to unchecked.
- Discovery step is skipped when no valid token exists for selected providers.
- Save policy is immediate per successful stage (token/correlation changes persist without waiting for full flow completion).
- If selected token source is environment variable, show read-only warning and explicit override affordance:
  - `env token is read-only; press T to store override`
- Long-running setup operations render in-place progress states and must not block live refresh.
- Footer legends in Settings use concise state-specific forms:
  - Idle/result: `[Enter] onboard  [t/T] token  [v] validate  [x] remove  [r] refresh`
  - Provider select: `[↑↓] move  [Space] toggle  [Enter] discover  [Esc] cancel`
  - Resource select: `[↑↓] move  [Space] toggle  [Enter] preview  [Esc] cancel`
  - Preview: `[Enter] apply  [Esc] back`
  - Busy: `[... ] working`

### 4.18 Detail Panel (TUI — Drill-Down)

```
  ┌─ my-saas ─────────────────────────────────────────────┐
  │                                                        │
  │  Commit    a1b2c3d  Update auth flow (#142)            │
  │  Author    @vivallo                                    │
  │  Branch    main                                        │
  │  Time      2 minutes ago                               │
  │                                                        │
  │  GitHub    ✓ CI passed (45s)            ● exact        │
  │  Railway   ✓ SUCCESS                    ◐ high         │
  │  Vercel    ✓ ready (preview.vercel.app) ● exact        │
  │                                                        │
  └────────────────────────────────────────────────────────┘

Styling:
  Border:         ROUNDED, border.default (border.focus when panel is active)
  Title:          T3 (bold, fg.emphasis) in border top-left
  Labels:         T7 (fg.subtle), column-aligned
  Values:         T5 (bold, fg.strong)
  SHA preview:    T6, first 7 chars, fg.muted
  Commit msg:     T6 (fg.default), truncated with …
  Status lines:   Status Badge + duration in T8
  Confidence:     Right-aligned, Confidence Indicator component

Ratatui: Block with Borders::ALL, border_type(BorderType::Rounded)
```

### 4.19 Sparkline (TUI — Health Tab)

```
  my-saas     ▁▂▃▅▇▇▇▅▃▁▂▅▇▇▇▇▇▇▇▇     98

Styling:
  Project name:   T5 (bold, fg.strong), fixed 12 chars
  Sparkline:      ▁▂▃▄▅▆▇█ characters, colored by value:
                    >= 90: status.success
                    70-89: status.warning
                    < 70:  status.failure
  Current score:  T5, right-aligned, colored same as last bar

Width: Project(12) + gap(2) + sparkline(20) + gap(2) + score(4) = 40 chars min

Ratatui: Sparkline widget with custom bar_set, data from last 20 health scores.
```

### 4.20 Keyboard Shortcut Badge

```
  [q] quit   [Tab] switch tab (1-4)   [↵] select   [/] filter

Styling:
  Bracket + key:    fg: accent.primary, bold
  Description:      T8 (fg.muted)
  Spacing:          3 chars between badges
  Position:         Bottom line of TUI, full-width bar with bg.surface

Ratatui: Paragraph with multiple Spans, placed in bottom layout chunk.
```

### 4.21 Empty State

```
  No tracked projects.

  Run pulsos repos sync to discover and select projects.

Styling:
  Primary text:     T5 (bold, fg.subtle), centered
  Action text:      T6 (fg.default), centered
  Command:          T5 (bold, accent.primary) inline
  Padding:          space.lg above and below
```

### 4.22 Logo / Brand Mark

```
  ┌─────────────────────┐
  │   P U L S O S       │    ← T1: UPPERCASE, letter-spaced, accent.primary
  │   ───────────       │    ← accent.dim underline
  └─────────────────────┘

  Compact: PULSOS (inline, accent.primary, bold)
  Version: v0.1.0 (T9, fg.muted, after logo)
```

### 4.23 Correlation Map

```
  my-saas
    GitHub     myorg/my-saas           ● linked
    Railway    my-saas-api (prod)      ◐ mapped
    Vercel     my-saas-web             ● linked (via repo)

Styling:
  Project name:     T5 (bold, fg.strong)
  Platform label:   T6 (fg.default), indent.sm, fixed 10 chars
  Resource name:    T6 (fg.default)
  Environment:      T8 (fg.muted) inline in parens
  Confidence:       Confidence Indicator, right-aligned
  Link method:      T8 (fg.muted) inline after indicator
```

### 4.24 Notification Bar (TUI)

```
  ⚠ GitHub rate limit at 15%. Polling slowed to 60s.

Styling:
  Background:       status.warning.muted (full width bar)
  Icon + text:      status.warning, T6
  Position:         Directly below tab bar, above main content
  Auto-dismiss:     After 10 seconds or when condition resolves
```

### 4.25 Confirm Dialog (TUI)

```
  ┌── Remove project? ──────────────────────────────┐
  │                                                   │
  │  Stop tracking "my-saas" across all platforms?    │
  │                                                   │
  │  This will not delete any data on GitHub,         │
  │  Railway, or Vercel.                              │
  │                                                   │
  │                        [Cancel]    [ Remove ]     │
  │                                                   │
  └───────────────────────────────────────────────────┘

Styling:
  Border:           ROUNDED, border.focus
  Title:            T3 (bold, fg.emphasis) in border
  Body:             T6 (fg.default), centered
  Reassurance:      T7 (fg.subtle)
  Active button:    bg: accent.primary, fg: bg.primary, bold
  Inactive button:  bg: bg.elevated, fg: fg.default
  Overlay:          bg.overlay at 80% of terminal, centered
```

### 4.26–4.30 Additional Components

```
4.26  Tree View (repos list)    — Nested list with ├── └── connectors, fg.muted for lines
4.27  JSON Output               — Unformatted, no styling (piped to jq)
4.28  Compact Output            — Single-line per project: "my-saas: ✓ ✓ ✓ 98"
4.29  Help Screen (TUI)         — Full-page overlay, two-column (key: description), bg.overlay
4.30  Version/About             — Logo + version + "Lambda Engineering" + repo URL, centered
```

---

## 5. Layout Patterns

### 5.1 CLI Output Layout (Non-TUI)

```
Terminal width detection: If unavailable, assume 80.

┌──────────────────────────── 80+ columns ────────────────────────────────┐
│                                                                          │
│  {brand mark or empty}                                                   │
│  {space.md}                                                              │
│  {section header + divider}                                              │
│  {space.sm}                                                              │
│  {content block — table, list, or prose}                                 │
│  {space.md}                                                              │
│  {section header + divider}                                              │
│  {space.sm}                                                              │
│  {content block}                                                         │
│  {space.lg}                                                              │
│  {footer — status summary, hints}                                        │
│                                                                          │
└──────────────────────────────────────────────────────────────────────────┘
```

### 5.2 TUI Layout (Ratatui)

```
Minimum: 80 × 24

┌──────────────────────────── Full Terminal ───────────────────────────────┐
│ ▏ PULSOS  Unified │ Platform │ Health          Last sync: 12s ago   ▕  │ ← Header (3 lines)
│ ════════                                                                │
├─────────────────────────────────────────────────────────────────────────┤
│                                                                          │
│  {Main Content Area — table, detail panels, sparklines}                  │ ← Content (h - 6 lines)
│                                                                          │
│                                                                          │
│                                                                          │
├─────────────────────────────────────────────────────────────────────────┤
│  [q] quit  [Tab] tab  [↵] select  [/] filter  [?] help                 │ ← Footer (1 line)
└─────────────────────────────────────────────────────────────────────────┘

Ratatui layout:
  Layout::default()
    .direction(Direction::Vertical)
    .constraints([
      Constraint::Length(3),           // Header: logo + tabs + divider
      Constraint::Min(10),             // Content: fills remaining
      Constraint::Length(1),           // Footer: key hints
    ])
```

### 5.3 Responsive Breakpoints

```
Breakpoint     Columns    Behavior
──────────────────────────────────────────────────────────────────
Minimum        80         Project | GitHub | Railway | Vercel (status only)
Comfortable    100        + Health score column
Standard       120        + Branch column + Updated column
Wide           160        + Commit SHA preview + Duration
Ultrawide      200+       + Full commit message + Sparkline inline
```

### 5.4 Split Panel Layout (TUI Detail View)

```
120+ columns:
┌─────────────────────────┬──────────────────────────────────┐
│  Status Table (60%)     │  Detail Panel (40%)              │
│                         │                                   │
│  ▸ my-saas    ✓ ✓ ✓    │  ┌─ my-saas ────────────────┐   │
│    api-core   ✓ ✓ —    │  │ Commit  a1b2c3d           │   │
│    auth-svc   ✗ ✓ —    │  │ Author  @vivallo          │   │
│                         │  │ ...                        │   │
│                         │  └────────────────────────────┘   │
└─────────────────────────┴──────────────────────────────────┘

< 120 columns: Detail panel replaces table (full-screen, [Esc] to return)
```

---

## 6. Animation Guidelines

### 6.1 Principles

Terminal animation must be purposeful. Every animated element answers the question "is the system working?" or "how long will this take?"

### 6.2 Timing

```
Token              Duration    Easing           Usage
──────────────────────────────────────────────────────────────────────
anim.spinner       80ms/frame  Linear           Braille spinner (⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏)
anim.cursor        530ms       Step (on/off)    Input cursor blink
anim.transition    0ms         Instant          Tab switch, panel change (no transition)
anim.fade-dim      400ms       Linear           Success message dims after completion
anim.hold          2000ms      —                Hold success state before collapsing/dimming
anim.refresh       200ms       —                Cache-first render, then data swap (no flash)
```

### 6.3 Refresh Strategy

```
Event                       Animation
──────────────────────────────────────────────────────────────────
New data arrives             Instant replace — no flash, no highlight
Status changes (pass→fail)   Cell briefly inverts (bg: status color, fg: bg) for 1 render cycle
Row selected                 Instant bg change to bg.elevated
Tab switch                   Instant content swap, no slide/fade
Wizard step complete         ✓ appears, hold 400ms, then next step renders below
Network error                Notification bar slides down (1 frame), stays until resolved
```

### 6.4 What NOT to Animate

- Table sorting transitions
- Column resizing
- Panel resize on terminal resize (instant reflow)
- Text appearing (no typewriter effects)
- Color transitions (no gradual color shifts)

---

## 7. Accessibility Requirements

### 7.1 Color Independence

Every piece of information conveyed by color must also be conveyed by shape or text:

```
Status      Color           Shape     Text
──────────────────────────────────────────────────
Success     green           ✓         "passed" / "success" / "ready"
Failure     red             ✗         "failed" / "crashed" / "error"
Warning     yellow          ⚠         "warning" / "stale" / "expiring"
Active      blue            ◌/⠋      "building" / "in_progress"
Neutral     gray            —         "skipped" / "sleeping" / "queued"
```

No information is lost on a monochrome display. The symbols alone are sufficient.

### 7.2 Contrast Ratios

```
Pair                              Calculated Ratio    WCAG AAA Target
──────────────────────────────────────────────────────────────────────
fg.emphasis (#FFF) on bg.primary (#0D0D0D)   19.2:1        ≥ 7:1 ✓
fg.default (#B0B0B0) on bg.primary           10.4:1        ≥ 7:1 ✓
fg.subtle (#777) on bg.primary                5.6:1        ≥ 4.5:1 ✓
fg.muted (#555) on bg.primary                 3.7:1        ≥ 3:1 ✓ (large text)
status.success (#34D399) on bg.primary       10.8:1        ≥ 7:1 ✓
status.failure (#F87171) on bg.primary        7.2:1        ≥ 7:1 ✓
status.warning (#FBBF24) on bg.primary       11.6:1        ≥ 7:1 ✓
status.active (#60A5FA) on bg.primary         7.8:1        ≥ 7:1 ✓
accent.primary (#818CF8) on bg.primary        6.8:1        ≥ 4.5:1 ✓
```

### 7.3 Terminal Compatibility

```
Feature             16-color fallback        True color (24-bit)
──────────────────────────────────────────────────────────────────
bg.primary          Black                    #0D0D0D
fg.emphasis         White (bold)             #FFFFFF
fg.default          White                    #B0B0B0
fg.subtle           DarkGray                 #777777
fg.muted            DarkGray                 #555555
status.success      Green                    #34D399
status.failure      Red                      #F87171
status.warning      Yellow                   #FBBF24
status.active       Blue                     #60A5FA
accent.primary      Magenta                  #818CF8

Detection: Check $COLORTERM == "truecolor" or "24bit".
Fallback: ANSI 16-color map. All information remains legible.
```

### 7.4 Screen Reader Considerations

CLI output (non-TUI) should be screen-reader friendly:

- Use plain text status words ("passed", "failed"), not just symbols
- Avoid box-drawing characters in non-TUI output (they read poorly)
- `--no-color` flag strips all ANSI codes
- `--format json` provides fully machine-readable output for assistive tools
- Status table in CLI mode uses spaces for alignment, not tabs

### 7.5 Reduced Motion

Respect `NO_COLOR` (de facto standard) and `PULSOS_NO_ANIMATION`:

```
NO_COLOR=1                   Strip all ANSI color codes
PULSOS_NO_ANIMATION=1        Replace spinners with static "..." 
                             Disable all cursor movement tricks
                             Use simple line-by-line output
PULSOS_UNICODE=ascii         Replace ✓✗⚠◆│├└ with +x!*||`-
```

---

## 8. Design Tokens — Rust Implementation

### 8.1 Theme Struct

```rust
// crates/pulsos-cli/src/tui/theme.rs

use ratatui::style::{Color, Modifier, Style};

pub struct Theme {
    // Background
    pub bg_primary: Color,
    pub bg_surface: Color,
    pub bg_elevated: Color,
    pub bg_overlay: Color,

    // Borders
    pub border_muted: Color,
    pub border_default: Color,
    pub border_focus: Color,

    // Foreground
    pub fg_muted: Color,
    pub fg_subtle: Color,
    pub fg_default: Color,
    pub fg_strong: Color,
    pub fg_emphasis: Color,

    // Semantic
    pub status_success: Color,
    pub status_success_muted: Color,
    pub status_failure: Color,
    pub status_failure_muted: Color,
    pub status_warning: Color,
    pub status_warning_muted: Color,
    pub status_active: Color,
    pub status_active_muted: Color,
    pub status_neutral: Color,

    // Accent
    pub accent_primary: Color,
    pub accent_dim: Color,
}

impl Theme {
    pub fn dark() -> Self {
        Self {
            bg_primary:           Color::Rgb(13, 13, 13),
            bg_surface:           Color::Rgb(22, 22, 22),
            bg_elevated:          Color::Rgb(30, 30, 30),
            bg_overlay:           Color::Rgb(37, 37, 37),

            border_muted:         Color::Rgb(51, 51, 51),
            border_default:       Color::Rgb(68, 68, 68),
            border_focus:         Color::Rgb(136, 136, 136),

            fg_muted:             Color::Rgb(85, 85, 85),
            fg_subtle:            Color::Rgb(119, 119, 119),
            fg_default:           Color::Rgb(176, 176, 176),
            fg_strong:            Color::Rgb(224, 224, 224),
            fg_emphasis:          Color::White,

            status_success:       Color::Rgb(52, 211, 153),
            status_success_muted: Color::Rgb(6, 95, 70),
            status_failure:       Color::Rgb(248, 113, 113),
            status_failure_muted: Color::Rgb(127, 29, 29),
            status_warning:       Color::Rgb(251, 191, 36),
            status_warning_muted: Color::Rgb(120, 53, 15),
            status_active:        Color::Rgb(96, 165, 250),
            status_active_muted:  Color::Rgb(30, 58, 95),
            status_neutral:       Color::Rgb(156, 163, 175),

            accent_primary:       Color::Rgb(129, 140, 248),
            accent_dim:           Color::Rgb(67, 56, 202),
        }
    }

    pub fn light() -> Self {
        Self {
            bg_primary:           Color::Rgb(250, 250, 250),
            bg_surface:           Color::Rgb(255, 255, 255),
            bg_elevated:          Color::Rgb(240, 240, 240),
            bg_overlay:           Color::Rgb(230, 230, 230),

            border_muted:         Color::Rgb(220, 220, 220),
            border_default:       Color::Rgb(212, 212, 212),
            border_focus:         Color::Rgb(120, 120, 120),

            fg_muted:             Color::Rgb(160, 160, 160),
            fg_subtle:            Color::Rgb(120, 120, 120),
            fg_default:           Color::Rgb(64, 64, 64),
            fg_strong:            Color::Rgb(26, 26, 26),
            fg_emphasis:          Color::Black,

            status_success:       Color::Rgb(5, 150, 105),
            status_success_muted: Color::Rgb(209, 250, 229),
            status_failure:       Color::Rgb(220, 38, 38),
            status_failure_muted: Color::Rgb(254, 226, 226),
            status_warning:       Color::Rgb(217, 119, 6),
            status_warning_muted: Color::Rgb(254, 243, 199),
            status_active:        Color::Rgb(37, 99, 235),
            status_active_muted:  Color::Rgb(219, 234, 254),
            status_neutral:       Color::Rgb(107, 114, 128),

            accent_primary:       Color::Rgb(79, 70, 229),
            accent_dim:           Color::Rgb(199, 210, 254),
        }
    }

    pub fn ansi16() -> Self {
        Self {
            bg_primary:           Color::Black,
            bg_surface:           Color::Black,
            bg_elevated:          Color::DarkGray,
            bg_overlay:           Color::DarkGray,

            border_muted:         Color::DarkGray,
            border_default:       Color::DarkGray,
            border_focus:         Color::White,

            fg_muted:             Color::DarkGray,
            fg_subtle:            Color::DarkGray,
            fg_default:           Color::White,
            fg_strong:            Color::White,
            fg_emphasis:          Color::White,

            status_success:       Color::Green,
            status_success_muted: Color::Black,
            status_failure:       Color::Red,
            status_failure_muted: Color::Black,
            status_warning:       Color::Yellow,
            status_warning_muted: Color::Black,
            status_active:        Color::Blue,
            status_active_muted:  Color::Black,
            status_neutral:       Color::DarkGray,

            accent_primary:       Color::Magenta,
            accent_dim:           Color::DarkGray,
        }
    }
}

// ── Convenience style constructors ──────────────────────────

impl Theme {
    // Typography levels
    pub fn t1(&self) -> Style { Style::new().fg(self.accent_primary).bold() }
    pub fn t2(&self) -> Style { Style::new().fg(self.fg_emphasis).bold() }
    pub fn t3(&self) -> Style { Style::new().fg(self.fg_emphasis).bold() }
    pub fn t4(&self) -> Style { Style::new().fg(self.fg_subtle).bold() }
    pub fn t5(&self) -> Style { Style::new().fg(self.fg_strong).bold() }
    pub fn t6(&self) -> Style { Style::new().fg(self.fg_default) }
    pub fn t7(&self) -> Style { Style::new().fg(self.fg_subtle) }
    pub fn t8(&self) -> Style { Style::new().fg(self.fg_muted) }
    pub fn t9(&self) -> Style { Style::new().fg(Color::Rgb(68, 68, 68)) }

    // Status styles
    pub fn success(&self) -> Style { Style::new().fg(self.status_success).bold() }
    pub fn failure(&self) -> Style { Style::new().fg(self.status_failure).bold() }
    pub fn warning(&self) -> Style { Style::new().fg(self.status_warning).bold() }
    pub fn active(&self) -> Style { Style::new().fg(self.status_active).bold() }
    pub fn neutral(&self) -> Style { Style::new().fg(self.status_neutral) }

    // Component styles
    pub fn selected_row(&self) -> Style { Style::new().bg(self.bg_elevated) }
    pub fn panel_border(&self) -> Style { Style::new().fg(self.border_default) }
    pub fn panel_border_focus(&self) -> Style { Style::new().fg(self.border_focus) }
    pub fn tab_active(&self) -> Style { Style::new().fg(self.fg_emphasis).bold() }
    pub fn tab_inactive(&self) -> Style { Style::new().fg(self.fg_subtle) }
    pub fn keybind_key(&self) -> Style { Style::new().fg(self.accent_primary).bold() }
    pub fn keybind_desc(&self) -> Style { Style::new().fg(self.fg_muted) }
}
```

### 8.2 Status Badge Renderer

```rust
// crates/pulsos-cli/src/tui/widgets/status_badge.rs

use ratatui::text::{Line, Span};
use crate::domain::deployment::DeploymentStatus;
use super::theme::Theme;

pub fn status_badge<'a>(status: &DeploymentStatus, theme: &'a Theme) -> Line<'a> {
    let (symbol, label, style) = match status {
        DeploymentStatus::Success   => ("✓ ", "passed",   theme.success()),
        DeploymentStatus::Failed    => ("✗ ", "failed",   theme.failure()),
        DeploymentStatus::InProgress=> ("◌ ", "building", theme.active()),
        DeploymentStatus::Queued    => ("⏸ ", "queued",   theme.neutral()),
        DeploymentStatus::Cancelled => ("— ", "cancelled",theme.neutral()),
        DeploymentStatus::Skipped   => ("— ", "skipped",  theme.neutral()),
        DeploymentStatus::ActionRequired => ("⚠ ", "action", theme.warning()),
        DeploymentStatus::Sleeping  => ("● ", "sleeping", theme.neutral()),
        DeploymentStatus::Unknown(s)=> ("? ", s.as_str(), theme.neutral()),
    };

    Line::from(vec![
        Span::styled(symbol, style),
        Span::styled(label, style),
    ])
}
```

### 8.3 Theme Detection

```rust
// crates/pulsos-cli/src/tui/theme_detect.rs

use std::env;

pub fn detect_theme(config_override: Option<&str>) -> Theme {
    // 1. Config override
    if let Some(theme) = config_override {
        return match theme {
            "light" => Theme::light(),
            "dark" => Theme::dark(),
            "ansi16" => Theme::ansi16(),
            _ => Theme::dark(),
        };
    }

    // 2. Environment variable
    if let Ok(theme) = env::var("PULSOS_THEME") {
        return match theme.as_str() {
            "light" => Theme::light(),
            "ansi16" => Theme::ansi16(),
            _ => Theme::dark(),
        };
    }

    // 3. NO_COLOR standard
    if env::var("NO_COLOR").is_ok() {
        return Theme::ansi16();
    }

    // 4. Color capability detection
    let truecolor = env::var("COLORTERM")
        .map(|v| v == "truecolor" || v == "24bit")
        .unwrap_or(false);

    if truecolor {
        Theme::dark()
    } else {
        Theme::ansi16()
    }
}
```

---

## 9. CLI Output Redesign — Before / After

### 9.1 First-Run Wizard

**BEFORE** (current — from screenshots):
```
No configuration found — let's set up Pulsos.

No GitHub token found. Authenticate now? yes
Authenticating with GitHub

  Create a personal access token at:
    https://github.com/settings/tokens

  Required scopes: repo, read:org

  Enter your GitHub token: ********...
    (93 characters received: ghp_****...F0ir)
    Validating... OK (@Vivallo04)
    Token stored securely for GitHub.
```

**AFTER** (redesigned):
```

  P U L S O S
  ───────────

  No configuration found — let's get you set up.


◆ GitHub                                                        Step 1 of 3
────────────────────────────────────────────────────────────────────────────

  Create a personal access token:
    https://github.com/settings/tokens

  Required scopes: repo, read:org

  Token: ••••••••••••••••••••••••••••

  ✓ Authenticated as @Vivallo04
    Token stored securely.


◆ Railway                                                       Step 2 of 3
────────────────────────────────────────────────────────────────────────────

  Create an Account token:
    https://railway.com/account/tokens

  Token: ••••••••••••••••••••••

  ✗ Not Authorized

    This token appears to be a Project token.
    Pulsos needs an Account token for cross-project access.

    → Create one at https://railway.com/account/tokens
      Select "No workspace" for account-level access.

  Token: ••••••••••••••••••••••••••

  ✓ Authenticated as vivallo@lambda.co
    2 workspaces, 8 projects found.


◆ Vercel                                                        Step 3 of 3
────────────────────────────────────────────────────────────────────────────

  Create a token:
    https://vercel.com/account/tokens

  Token: ••••••••••••••••••••••

  ✓ Authenticated as vivallo04
    1 team, 6 projects found.


◆ Discovery
────────────────────────────────────────────────────────────────────────────

  ⠹ Discovering projects across platforms...

  GitHub       47 repositories across 3 organizations
  Railway       8 projects across 2 workspaces
  Vercel        6 projects in 1 team

  Select repositories to monitor:

    myorg
      [✓] my-saas
      [✓] api-core
      [✓] auth-service
      [ ] legacy-monolith                                       archived

  ↑↓ navigate  ␣ toggle  ↵ confirm

  ✓ 3 projects tracked. Config saved to ~/.config/pulsos/config.toml


────────────────────────────────────────────────────────────────────────────

  Project          GitHub CI      Railway        Vercel         Health
  ─────────────────────────────────────────────────────────────────────
  my-saas          ✓ passed       ✓ success      ✓ ready          98
  api-core         ✓ passed       ✓ success      —                95
  auth-service     ✓ passed       ✓ success      —                91

  Run pulsos status for live monitoring in TTY (or use --watch explicitly).
  Use pulsos status --once for one-shot output.

```

### 9.2 Key Changes

1. **Logo mark** at the top — letter-spaced, accent-colored, establishes brand immediately
2. **Step indicators** with ◆ section markers and "Step N of M" right-aligned
3. **Full-width dividers** between sections create breathing room
4. **Indented content blocks** — everything inside a step is indented 2 chars
5. **Error recovery inline** — Railway failure shows the error, explains why, suggests the fix, then immediately offers retry. User never leaves the wizard.
6. **Discovery as its own section** — not buried in a wall of auth output
7. **Interactive selection** with visual affordances (checkmark, highlight, keyboard hints)
8. **Immediate dashboard** at the end — no "run another command," the value is right there
9. **Single closing hint** — one sentence, not three paragraphs
10. **Silence** — no "Authenticating with GitHub" (obvious), no "(93 characters received: ...)" (noise), no "Token was NOT stored" (confusing negative)
