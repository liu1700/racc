# UI Design

[< Home](Home.md) | [< Feature Specification](Feature-Specification.md) | [Cognitive Design Research](Cognitive-Design-Research.md)

> UI decisions in this document are grounded in the cognitive science research documented in [Cognitive Design Research](Cognitive-Design-Research.md). Key constraints: working memory holds 4±1 chunks, attention switching costs 100–500ms per switch, and vigilance degrades after 15 minutes of passive monitoring.

## Layout Overview

Three-panel layout, left to right:

```
+----------------+------------------------------------+----------------------+
|                |                                    |                      |
|  Left Sidebar  |         Center Main Area           |   Right Panel        |
|  (~15%)        |         (~55%)                     |   (~30%)             |
|                |                                    |                      |
|  Session List  |  Agent Terminal (PTY / xterm.js)   |  Activity Log        |
|  + Quick       |  ── or ──                          |  Cost Dashboard      |
|    Actions     |  Diff Review View                  |  File Change List    |
|                |  (switchable)                      |                      |
|  [New]         |                                    |                      |
|  [Pause]       |                                    |                      |
|  [Stop]        |                                    |                      |
|                |                                    |                      |
+----------------+------------------------------------+----------------------+
|                        Global Status Bar                                   |
|  Total Cost: $X.XX | This Week: $X.XX | Quota: XX% | Active Sessions: N   |
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
- Each session shows: agent type, branch name, status color dot, elapsed time (e.g., "12m", "2h 15m")
- **Status sorting (implemented):** Within each repo, sessions are sorted by status priority: error (0) → disconnected (1) → running (2) → completed (3) — so "needs attention" items always appear at the top
- **Running status pulse (implemented):** Running session dots use a subtle opacity animation (2s cycle) for ambient activity indication without distraction
- **Hover transitions (implemented):** All interactive elements use `transition-colors duration-150` for smooth visual feedback
- Quick actions per repo: [+] Launch new session, [×] Remove repo
- Quick actions per session: Stop (if running), Remove (if not running)
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

## Center Main Area — Terminal (implemented)

Currently terminal-only mode:

### Terminal Mode (default — Deep Work)
- Full xterm.js 5.5 terminal rendering the active agent session
- Dark theme: background `#1a1a1f`, foreground `#d4d4d8`, cursor `#6366f1` (indigo accent)
- FitAddon for responsive sizing with ResizeObserver
- Input goes directly to the agent via PTY write
- Buffer replay on session switch (up to 1MB per session)
- Async dynamic import of xterm to avoid blocking initial render
- Placeholder message when no active session selected

### Diff Review Mode *(planned)*
- Placeholder component exists (`DiffViewer.tsx`)
- Backend `get_diff` command returns `git diff HEAD` output
- Full side-by-side review UI planned for P1
- **Batched review design:** When agents complete work, diffs queue for review. The developer enters review mode on their own schedule — no forced interruption of deep work. Aligns with research showing optimal review at 200–400 lines per session, with effectiveness dropping after 60–90 minutes.

## Right Panel — Intelligence Dashboard

### Cost Dashboard (implemented)
- Polls `get_project_costs` every 10 seconds
- Displays: total estimated cost, session count
- Token breakdown: input, output, cache creation, cache read tokens
- Model-aware pricing: Opus ($15/$75), Sonnet ($3/$15), Haiku ($0.80/$4) per 1M tokens
- Silent failure if cost data is unavailable

### Activity Log *(placeholder)*
- Shows "Agent activity will appear here"
- Structured event parsing planned for P1
- **Progressive disclosure:** Summary view shows one-line-per-event for information scent; expanding an event reveals full detail. Prevents cognitive overload while maintaining transparency.

### File Change List *(planned)*
- Not yet implemented
- Will show files modified in current session with status badges

## Global Status Bar (implemented)

Fixed bottom bar showing:
- **Categorical session summary (implemented):** Color-coded counts by status category (e.g., "2 running · 1 error · 1 completed") with status-colored numbers — only non-zero categories shown. Enables the developer to hold system state as categorical chunks rather than N individual items.
- Connection status indicator (green dot)
- Placeholder cost displays (to be connected to real-time aggregation)

## Notification Architecture

A five-tier alert system designed to prevent alarm fatigue (healthcare data shows 72–99% false alarm rates cause dangerous desensitization):

| Tier | Type | Implementation | Interruption |
|------|------|----------------|--------------|
| **1 — Ambient** | Status indicators | Color dot per session in sidebar | None — preattentive |
| **2 — Informational** | Progress updates | Subtle border pulse on session card | None — peripheral |
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
