# Feature Specification

[< Home](Home.md) | [< Product Vision](Product-Vision.md)

## P0: Must-Have (MVP)

These features define the minimum viable product. Without any one of them, the product doesn't solve the core problem.

### 1. Multi-Session Dashboard

The main interface showing all active agent sessions as status cards. Designed around Cowan's working memory limit of 4±1 chunks — see [Cognitive Design Research](Cognitive-Design-Research.md).

**Each card = one cognitive chunk:**
- Status color dot (preattentive pop-out for instant triage)
- Current task description / micro-summary
- Runtime duration and progress indicator
- Token consumption (input/output breakdown)
- Latest terminal output (truncated) displayed inline in the sidebar — **implemented**: real-time PTY output capture per session
- Associated git branch and worktree path

**Categorical grouping:** Sessions are ordered by status priority (error/blocked → running → completed) so "needs attention" items always surface first. When 10 agents are grouped into 3 status categories, the developer holds 3 chunks rather than 10.

**Key actions:**
- One-click new session creation (auto-creates worktree + spawns PTY + starts agent)
- Stop / terminate sessions
- Quick switch between sessions (with PTY output buffer replay)

### 2. Real-Time Token Usage Tracking

This is the **#1 user pain point** — the community has independently built 7+ monitoring tools, proving urgency.

**Per-session:**
- Token consumption (input/output breakdown)

**Global (status bar):**
- Total tokens across all sessions
- Weekly token usage
- Session count by status

**MVP approach:** Read Claude Code's local usage data files. Token counts only — no USD cost estimation (irrelevant for subscription users like Claude Max). Support for other agents' usage data in later versions.

### 3. Visual Diff Review *(not yet implemented)*

When an agent completes a round of work, provide a proper review experience. Designed around the cognitive research finding that review effectiveness drops after 60–90 minutes and 200–400 LOC — see [Cognitive Design Research](Cognitive-Design-Research.md).

**Features:**
- Side-by-side diff view (GitHub PR review style)
- Per-file accept / reject
- Checkpoint timeline — roll back to any historical point
- File change list with status indicators (added / modified / deleted)

**Batched review model:** Completed agent work queues for review. The developer enters review mode on their own schedule rather than being interrupted mid-flow. This resolves the flow-monitoring paradox: agents run in the background (deep work mode) → results accumulate → developer surfaces for active evaluation (monitoring mode).

**Why this matters:** "Blindly accepting changes" is a real danger — Parasuraman's research shows complacency risk increases with automation reliability. Users need a review gate between agent output and their codebase, and the review experience must support active comprehension, not rubber-stamping.

**Current status:** `get_diff` Rust command exists (returns `git diff HEAD`). UI placeholder exists in `DiffViewer.tsx`. Full review UI is planned for P1.

### 4. Insights Panel — Cross-Session Pattern Detection (implemented)

An actionable insights feed that replaces the previous AI assistant chat panel. Instead of a generic chat, it automatically detects patterns across sessions and surfaces one-click suggestions to accelerate development workflows.

**Six insight types:**

| Type | Trigger | Severity |
|------|---------|----------|
| Repeated Prompt | Same/similar instruction in ≥3 sessions | Warning (amber) |
| Startup Pattern | ≥3 sessions begin with same command sequence | Warning (amber) |
| Repeated Permission | Same permission type requested ≥3 times in one session | Warning (amber) |
| Cost Anomaly | 10-min cost > 3× session's historical average | Alert (red) |
| File Conflict | Same file written/edited in ≥2 active sessions | Alert (red) |
| Similar Sessions | Two sessions share overlapping file sets | Suggestion (green) |

**Architecture:** Hybrid frontend/backend analysis. Frontend runs real-time rules (file conflicts, cost anomalies, permission repeats) on structured events captured from PTY output. Rust backend runs batch analysis every 5 minutes (repeated prompt clustering via Levenshtein similarity, startup pattern detection, similar session detection via Jaccard index). LLM is used only for generating suggestion text (e.g., CLAUDE.md entries), never for detection.

**Event capture:** `ptyOutputParser.ts` is extended to extract user prompts (❯ marker detection). `eventCapture.ts` buffers events and flushes to SQLite every 30 seconds. Events are also fed to real-time rules in the `insightsStore`.

**UI:** Chronological timeline feed with severity-colored dots. Cards expand inline to show evidence (matched prompts, conflicting files, etc.) and action buttons. Actions include: "Add to CLAUDE.md", "Copy allowlist rule", "Switch to session", "View File", "Dismiss".

**Deduplication:** Insights have a fingerprint column with a unique partial index on active status, preventing duplicate detections across batch runs.

**Current status:** Fully implemented. Components: `InsightsPanel.tsx`, `InsightCard.tsx`, `InsightActions.tsx`. State: `insightsStore.ts`. Services: `eventCapture.ts`. Backend: `insights.rs` (event recording, insight CRUD, batch analysis). Settings gear opens `AssistantSetup.tsx` for LLM API key configuration (used for generating suggestion text).

### 5. Task Board — Cognitive Offloading & Agent Orchestration (implemented)

A kanban-style task board integrated into the center panel for cognitive offloading and automated agent orchestration. Users write task descriptions (the act of writing IS the cognitive offload) and "fire" them to automatically spawn agent sessions.

**Lifecycle:** Open → Working → Closed

- **Open:** User writes a task description via inline textarea (multiline, wraps) — zero-config, minimal friction. Open tasks are editable: click the description to inline-edit before firing
- **Working:** System auto-creates a session (worktree + PTY), sends task description as initial prompt. Card shows real-time agent activity via PTY Output Parser (information scent)
- **Closed:** Session completes or is removed → task automatically moves to Closed

**Fire Dialog:** Reuses NewAgentDialog pattern — agent selection, skip-permissions, worktree (ON by default), auto-generated branch name (`task/keywords` from description).

**UI Integration:**
- Center panel has Tasks | Terminal tab switching (Tasks is default view)
- Terminal stays mounted via CSS `hidden` to preserve xterm.js state
- Firing a task keeps user on Task Board; clicking sidebar session switches to Terminal
- Task count badge on Tasks tab
- Draft input state (open/closed + text) persists across tab switches via Zustand store

**Cognitive design rationale:** Based on Risko & Gilbert's cognitive offloading research — externalizing working memory to the task board reduces cognitive load. Batched evaluation (Review column) supports 60–90 minute work cycles per the cognitive research. Preattentive color coding (green=running, amber=review, blue=done) for <200ms status recognition.

**Data model:** `tasks` table (SQLite v4) with FK to `repos` (CASCADE) and `sessions` (SET NULL). Status CHECK constraint enforces valid values.

**Current status:** Fully implemented. Components: `TaskBoard/` (TaskBoard, TaskColumn, TaskCard, TaskInput, FireTaskDialog). Store: `taskStore.ts`. Backend: `task.rs` (create, list, update_status, update_description, delete).

### 6. File Viewer & Command Palette (implemented)

A zero-footprint file viewer designed around cognitive science principles — no persistent file tree, no extra tabs. Files are viewed on demand and the UI disappears completely when closed.

**Three trigger mechanisms (all funnel through a single `openFile()` action):**
1. **Cmd+P command palette** — fuzzy file search powered by `nucleo-matcher`, respects `.gitignore` via `ignore` crate
2. **Terminal path click** — Cmd+Click on file paths detected in xterm.js terminal output (regex-based link provider)
3. **Pi Agent `read_file` tool** — assistant can read files on the user's behalf, showing a lightweight inline preview (≤30 lines) with an "Open Full File" button

**Full overlay viewer:**
- Shiki syntax highlighting (`github-dark-default` theme, VS Code-compatible TextMate grammars)
- Cmd+F in-file search with match count, navigation (Enter/Shift+Enter), and current match highlighting
- Ctrl+G jump-to-line
- Layered Esc dismiss (search → viewer → close)
- Click-to-highlight line
- 95% opacity overlay to avoid figure-ground interference while keeping the sidebar visible for preattentive status monitoring
- Bottom status strip showing branch, session status, and elapsed time

**Cognitive design rationale:** Progressive disclosure (Information Foraging Theory) — the developer sees only what they need, when they need it. The command palette provides "information scent" through fuzzy matching scores. The dual-mode design (inline preview vs. full overlay) respects Cowan's 4±1 working memory limit by not overwhelming with content.

**Current status:** Fully implemented. Components: `FileViewer.tsx`, `CommandPalette.tsx`. State: `fileViewerStore.ts`. Backend: `file.rs` (`read_file`, `search_files` commands).

---

## P1: Important, Deferred

These features significantly enhance the product but are not required for initial validation.

| Feature | Description | Dependency |
|---------|-------------|------------|
| **Task Queue Enhancements** | Task dependencies, priority ordering, bulk operations, drag-and-drop | Task Board (implemented) |
| **Tiered Notification System** | Five-tier alerts (ambient → critical) with anti-fatigue design: deduplication, notification budgets, user thresholds. Audio channel for Tier 3+ per Wickens' Multiple Resource Theory | Session status tracking |
| **Cross-Machine Session Management** | Connect to remote agent sessions via Tailscale, manage from one dashboard | Tailscale integration |
| **Portless Integration** | Auto-assign named URLs per worktree, embed preview window in IDE | Portless setup |
| **Multi-Agent Conflict Detection** | ~~Warn when multiple agents modify the same file~~ **Implemented** via Insights Panel file conflict detection | ~~File change tracking~~ Done |

---

## P2: Future Vision

Lower priority — depends on ecosystem maturity.

| Feature | Description | Blocker |
|---------|-------------|---------|
| **Visual Regression Review** | Screenshot comparison, browser preview | Requires mature agent capabilities |
| **Spec-Driven Development** | Built-in requirements.md / tasks.md editor tied to agent execution | Workflow design needed |
| **Global Knowledge Base** | Cross-session CLAUDE.md management and sync | Multi-session maturity |

[Next: UI Design >](UI-Design.md)
