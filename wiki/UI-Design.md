# UI Design

[< Home](Home.md) | [< Feature Specification](Feature-Specification.md) | [Cognitive Design Research](Cognitive-Design-Research.md)

> UI decisions in this document are grounded in the cognitive science research documented in [Cognitive Design Research](Cognitive-Design-Research.md). Key constraints: working memory holds 4±1 chunks, attention switching costs 100–500ms per switch, and vigilance degrades after 15 minutes of passive monitoring.

## Layout Overview

Three-panel layout, left to right:

```
+----------------+------------------------------------+----------------------+
|                |                                    |                      |
|  Left Sidebar  |              Center Main Area                            |
|  (~15%)        |              (~85%)                                       |
|                |                                                          |
|  Session List  |  Tasks / Terminal (tab switching)                         |
|  + Inline      |  ── or ──                                                 |
|    Activity    |  Diff Review View                                         |
|  + Quick       |  (switchable)                                             |
|    Actions     |                                                           |
|                |                                                           |
+----------------+------------------------------------+----------------------+
|                        Global Status Bar                                   |
|  Sessions: 2 running | Total Tokens: XX.Xk | This Week: XX.Xk             |
+----------------------------------------------------------------------------+
```

**F-pattern scanning alignment:** The left sidebar (session list) occupies the highest-priority upper-left position, matching natural eye-scanning patterns. The center terminal is the primary interaction surface. The right panel provides supplementary intelligence without competing for primary attention.

## Critical Design Decision

**The agent terminal occupies the center main area** — not a sidebar.

This is a deliberate departure from Cursor/Windsurf, which squeeze agents into side panels. Racc's users are migrating from full-screen terminal agent workflows. The terminal must remain the primary interaction surface.

## Cognitive Design Principles

These principles are derived from the [Cognitive Design Research](Cognitive-Design-Research.md) and inform every UI decision below.

### 1. Categorical Chunking Over Individual Tracking

Managing 10 agents should feel like managing 3 status categories, not 10 individual items. The sidebar groups sessions by status category (needs attention → running normally → completed) so working memory holds categorical chunks within Cowan's 4±1 limit.

Each session card compresses into **one cognitive chunk**: status color + task description + progress indicator + time elapsed — readable without opening a detail view.

### 2. Information Scent for Rapid Triage

Every session card provides enough "information scent" (Pirolli & Card) for the developer to decide whether to investigate without opening a detail view:

- Status color (preattentive pop-out)
- Micro-summary (e.g., "Refactoring auth.py — 2/3 tests passing — 73%")
- Time elapsed since last meaningful progress
- Error count badge (if any)

### 3. Mode Separation: Monitoring vs. Deep Work

The IDE supports two cognitive modes to resolve the flow-monitoring paradox:

- **Deep work mode (default):** Developer focuses on one agent's terminal or their own code. Other agents run in the background. Completed work queues for batched review.
- **Monitoring mode:** Overview of all sessions via the sidebar's categorical status grouping. Designed for periodic check-ins, not continuous surveillance.

The developer should spend most time in deep work and periodically surface into monitoring mode for evaluation.

### 4. Active Engagement Over Passive Surveillance

Research shows passive monitoring degrades vigilance within 15 minutes, but active micro-engagement preserves it. The IDE should never ask developers to passively watch progress bars. Instead:

- Agents pause at meaningful decision points for human input
- Review queues accumulate completed work for active evaluation sessions
- The batched review cycle transforms monitoring from passive surveillance into active assessment

### 5. Preattentive Visual Encoding

Status uses a **single preattentive channel** (color hue) so problems pop out automatically across all sessions in under 200ms. No status requires conjunction search (checking two attributes together).

## Left Sidebar — Session List (implemented)

- Expandable repo list with nested sessions underneath each repo
- Each repo shows: name, path, expand/collapse toggle
- Each session shows: agent type, branch name, status color dot, elapsed time (e.g., "12m", "2h 15m"), and a second line with truncated latest terminal output
- **Status sorting (implemented):** Within each repo, sessions are sorted by status priority: error (0) → disconnected (1) → running (2) → completed (3) — so "needs attention" items always appear at the top
- **Running status pulse (implemented):** Running session dots use a subtle opacity animation (2s cycle) for ambient activity indication without distraction
- **Hover transitions (implemented):** All interactive elements use `transition-colors duration-150` for smooth visual feedback
- Quick actions per repo: [+] New task (switches to Task Board with input ready), [×] Remove repo
- Quick actions per session:
  - Running: [■] Stop session
  - Not running: [▶] Reattach session (re-spawn PTY with `claude --continue`), [×] Remove session (with confirmation dialog; worktree sessions offer optional `git worktree remove`)
- Import Repo button opens native folder picker

### Status Colors

Designed to align with color psychology research — reserving high-arousal red for true errors only, using calming tones for normal states:

| Status | Color | Hex | Rationale |
|--------|-------|-----|-----------|
| Running | Green | `#22c55e` | Active and healthy — green reduces stress (cortisol -53%) |
| Completed | Blue | `#3b82f6` | Calm completion signal — blue reduces autonomic arousal |
| Waiting/Paused | Amber | `#f59e0b` | Needs attention but not urgent — moderate arousal |
| Disconnected | Orange | `#f97316` | Anomalous state requiring investigation |
| Error | Red | `#ef4444` | True error only — reserved for high-urgency preattentive pop-out |

**Constraint:** Status is communicated via color hue alone (single preattentive channel). Shape, size, and position encode other dimensions — never combined with color to indicate status.

## Center Main Area — Tasks / Terminal (implemented)

The center panel has a tab bar switching between **Tasks** (default) and **Terminal** views. Terminal stays mounted via CSS `hidden` to preserve xterm.js state across tab switches.

### Task Board Mode (default — Planning & Monitoring)

A kanban-style board for cognitive offloading and agent orchestration. Three columns: Open, Working, Closed.

- **Open column:** Tasks awaiting execution. Inline "+ New Task" input at bottom — type description, press Enter. Each card has a "Fire" button
- **Working column:** Tasks with active agent sessions. Cards show real-time agent activity via PTY Output Parser — green pulsing dot + branch name + current action (information scent). Elapsed time display. Branch name and live output are rendered as separate truncatable spans to prevent layout shifts from rapid PTY updates
- **Closed column:** Session completed or removed — archived tasks at reduced opacity

**Fire flow:** Click Fire → modal dialog (reuses NewAgentDialog pattern) with agent selection, skip-permissions, worktree ON by default, auto-generated branch name (`task/keywords`). Firing stays on Task Board; new session appears in sidebar.

**Tab badge:** Tasks tab shows count of non-closed tasks in a rounded badge.

**Layout stability:** Task board uses CSS Grid (`grid-cols-3`) instead of flexbox for column layout, ensuring columns maintain fixed equal widths regardless of content changes. Each card uses `overflow-hidden` and `min-w-0` to prevent live output text from pushing column boundaries. This eliminates width glitching caused by rapid `sessionLastOutput` updates in working cards.

**Cognitive design:** Writing a task IS cognitive offloading (Risko & Gilbert). Preattentive color coding per column (accent=open, green=working, muted=closed).

### Terminal Mode (Deep Work)
- Full xterm.js 5.5 terminal rendering the active agent session
- Dark theme: background `#1a1a1f`, foreground `#d4d4d8`, cursor `#6366f1` (indigo accent)
- FitAddon for responsive sizing with ResizeObserver
- Input goes directly to the agent via PTY write
- Buffer replay on session switch (up to 1MB per session)
- Async dynamic import of xterm to avoid blocking initial render
- Placeholder message when no active session selected
- **Chinese IME compatibility:** `usePtyBridge` intercepts Shift+punctuation at the `keydown` level, bypassing IME mode-switching to ensure characters like `?`, `!`, `@` are correctly sent to the PTY

### File Viewer Mode (implemented)

A zero-footprint overlay for viewing source code and documentation — appears on demand, disappears completely when closed. Designed around progressive disclosure (Information Foraging Theory) so developers see only what they need before deciding what instructions to give agents.

**Triggers:**
- **Cmd+P** — Opens the command palette for fuzzy file search (global shortcut)
- **Cmd+Click on terminal paths** — xterm.js link provider detects file path patterns and opens the file with optional line scroll
- **Pi Agent `read_file` tool** — Assistant reads files inline (≤30 lines) with an "Open Full File" button to launch the overlay

**Overlay design:**
- Positioned as `absolute inset-0 z-30` within the center `<main>` panel (sidebar remains visible for preattentive status monitoring)
- 95% opacity (`bg-surface-0/95`) to avoid figure-ground interference
- 150ms fade transition for smooth appearance/disappearance
- Shiki syntax highlighting with `github-dark-default` theme and CSS counter-based line numbers
- Top bar: file path, line count, language, encoding, truncation indicator
- Bottom status strip: branch, session status, elapsed time

**Keyboard shortcuts:**

| Shortcut | Action |
|----------|--------|
| `Cmd+P` | Open command palette (fuzzy file search) |
| `Cmd+F` | Open in-file search (when overlay is open) |
| `Ctrl+G` | Jump to line number |
| `Enter` / `Shift+Enter` | Navigate to next/previous search match |
| `Esc` | Layered dismiss: search bar → viewer → close |

**Command palette:**
- Fixed overlay (`fixed inset-0 z-40`) covering the entire viewport
- Fuzzy matching via `nucleo-matcher` with 100ms debounced search
- Keyboard navigation: Arrow keys to select, Enter to open, Esc to close
- Respects `.gitignore` via the `ignore` crate

### Diff Review Mode *(planned)*
- Placeholder component exists (`DiffViewer.tsx`)
- Backend `get_diff` command returns `git diff HEAD` output
- Full side-by-side review UI planned for P1
- **Batched review design:** When agents complete work, diffs queue for review. The developer enters review mode on their own schedule — no forced interruption of deep work. Aligns with research showing optimal review at 200–400 lines per session, with effectiveness dropping after 60–90 minutes.

## Right Panel — Insights Panel (hidden for MVP)

> **Status:** UI hidden and all event capture/analysis disabled for MVP. Code is preserved in the codebase (`InsightsPanel.tsx`, `InsightCard.tsx`, `InsightActions.tsx`, `insightsStore.ts`, `eventCapture.ts`, `insights.rs`) for future re-enablement. The center panel now takes the full remaining width after the sidebar.

An actionable insights feed that automatically detects patterns across sessions and surfaces one-click suggestions. Designed to replace the previous AI assistant chat panel — instead of requiring users to manually ask questions, the panel proactively identifies workflow optimizations.

<details>
<summary>Full design (collapsed — not active in MVP)</summary>

### Timeline Feed
- Chronological list of detected insights, newest first
- Each insight rendered as a card with severity-colored left border and timeline dot
- Severity colors: red (alert — file conflicts, cost spikes), amber (warning — repeated prompts, permissions), green (suggestion — similar sessions)
- Cards expand inline on click to reveal evidence and action buttons
- Empty state: "No insights yet. Patterns will appear as you work across sessions."
- Active count badge in header

### Real-Time Detection (frontend)
Three rules run on every incoming event with zero delay:
- **File Conflict:** Tracks `Map<filePath, Set<sessionId>>` — alerts when >1 session edits the same file
- **Cost Anomaly:** Rolling window of 10 cost deltas per session — alerts when latest > 3× average and > $0.50
- **Repeated Permission:** Per-session permission counter — warns at ≥3 of the same type

### Batch Detection (Rust backend, every 5 minutes)
Three detectors run on persisted event history:
- **Repeated Prompt:** Clusters user inputs from last 7 days using normalized Levenshtein similarity (threshold ≥0.7, `strsim` crate). Triggers when ≥3 distinct sessions share a cluster.
- **Startup Pattern:** Compares first 5 user inputs per session. Triggers when ≥3 sessions share the same command prefix.
- **Similar Sessions:** Compares file operation sets across running sessions using Jaccard index (threshold ≥0.4).

Results pushed to frontend via Tauri event (`insight-detected`).

### Insight Actions
Each card expands to show evidence (matched prompts, conflicting files, session details) and type-specific action buttons:

| Type | Primary Action | Secondary |
|------|---------------|-----------|
| Repeated Prompt | Add to CLAUDE.md | Dismiss |
| Startup Pattern | Add to CLAUDE.md | Dismiss |
| Repeated Permission | Copy allowlist rule | Dismiss |
| Cost Anomaly | Switch to session | Dismiss |
| File Conflict | View File | Switch to session, Dismiss |
| Similar Sessions | Switch to session | Dismiss |

### Settings
Settings gear (⚙) in the panel header opens `AssistantSetup.tsx` for configuring API keys — needed for LLM-generated suggestion text (e.g., summarizing repeated prompts into CLAUDE.md entries). Insights still detect and display without an API key; suggestion text shows raw evidence instead.

### Event Capture Pipeline
`ptyOutputParser.ts` detects user prompts (❯ marker) and agent activities. `eventCapture.ts` normalizes events and routes them to: (1) `insightsStore` for real-time rules, (2) SQLite via batched flush every 30 seconds. Events include: `user_input`, `file_operation`, `permission_request`, `cost_update`, `session_meta`.

### Deduplication
Each insight has a fingerprint (e.g., sorted session IDs + matched text hash). A unique partial index (`WHERE status = 'active'`) prevents duplicate insights at the database level.

</details>

## Inline Session Activity (implemented)

Each session item in the sidebar shows a second line with the latest terminal output, providing at-a-glance awareness of what each agent is doing without a separate panel.

**Design:**
- Status color dot + branch name + elapsed time (first row)
- Truncated latest terminal output in muted text (second row, `text-[10px] text-zinc-600`)
- Running sessions always reserve a fixed-height line (`h-3.5`) for output to prevent height jumps when output starts/stops

**PTY output capture:** A frontend service (`ptyOutputParser.ts`) subscribes to each running session's PTY output via `ptyManager.subscribe()`, strips ANSI escape sequences, and captures the last non-empty line (truncated to 120 chars). Stored as `sessionLastOutput: Record<number, string>` in the Zustand store.

**Implementation:** `ptyOutputParser.ts` service, `sessionLastOutput` state in `sessionStore.ts`, inline display in `Sidebar.tsx`. Lifecycle hooks wire `startTracking` after `spawnPty` and `stopTracking` before `killPty`.

## Global Status Bar (implemented)

Fixed bottom bar showing:
- **Categorical session summary (implemented):** Color-coded counts by status category (e.g., "2 running · 1 error · 1 completed") with status-colored numbers — only non-zero categories shown. Enables the developer to hold system state as categorical chunks rather than N individual items.
- **Token usage (implemented):** Total Tokens and This Week counts, polled from `get_project_costs` every 10s
- Connection status indicator (green dot)

## Notification Architecture

A five-tier alert system designed to prevent alarm fatigue (healthcare data shows 72–99% false alarm rates cause dangerous desensitization):

| Tier | Type | Implementation | Interruption |
|------|------|----------------|--------------|
| **1 — Ambient** | Status indicators | Color dot per session in sidebar | None — preattentive |
| **2 — Informational** | Progress updates | Inline terminal output in sidebar (truncated last line) | None — peripheral |
| **3 — Advisory** | Task complete | Non-blocking toast with soft chime | Low |
| **4 — Warning** | Error/blocked | Persistent amber banner + distinctive tone | Medium |
| **5 — Critical** | Security/data loss | Modal overlay + urgent sound | High |

**Anti-fatigue rules:**
- Signal-to-noise target above 50% — aggregate similar issues across agents
- Notification budget per time window prevents alert storms
- User-configurable thresholds per tier
- Auditory channel for Tier 3+ (Wickens' Multiple Resource Theory: audio doesn't compete with visual code reading)

## Typography

- **JetBrains Mono** for all code display — designed with increased x-height for readability at small sizes, critical when displaying code across multiple simultaneous panels
- Minimum **13px** for code in small panels, **14px** in the main terminal
- Line-height **1.4–1.5** for code blocks
- Font size matters more than font choice for readability (Rello & Pielot, 2016)

## Dark Mode Design

- **Default: dark mode** — matches 70% developer preference, produces lower perceived workload in eye-tracking studies
- **Background (implemented):** Dark gray palette — `surface-0: #121215`, `surface-1: #1a1a1f`, `surface-2: #232329`, `surface-3: #2e2e35` (never pure `#000000` — causes halation/eye strain)
- **Text (implemented):** Muted white `#d4d4d8` (not pure `#FFFFFF` — reduces glare in extended sessions)
- **Light mode toggle required** — approximately 50% of the population has astigmatism, where light-on-dark text causes visual artifacts. Also needed for bright ambient conditions and users with dyslexia.
- Light mode uses positive polarity (dark text on light background) for better visual acuity and proofreading accuracy

## Automation Level Indicators *(planned)*

Different task types warrant different levels of human oversight (Parasuraman-Sheridan-Wickens framework). The UI should communicate the expected automation level per session:

| Level | Label | Behavior | Visual Indicator |
|-------|-------|----------|-----------------|
| High autonomy | "Auto" | Agent executes, informs afterward | Muted status, minimal attention needed |
| Approval gate | "Review" | Agent pauses at decisions for human approval | Amber pulse when waiting |
| Collaborative | "Paired" | Agent suggests, human selects | Active attention indicator |

This helps developers calibrate trust appropriately — knowing which sessions to scrutinize closely vs. which to let run.

[Next: Technical Architecture >](Technical-Architecture.md)
