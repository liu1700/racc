# Feature Specification

[< Home](Home.md) | [< Product Vision](Product-Vision.md)

## P0: Must-Have (MVP)

These four features define the minimum viable product. Without any one of them, the product doesn't solve the core problem.

### 1. Multi-Session Dashboard

The main interface showing all active agent sessions as status cards. Designed around Cowan's working memory limit of 4±1 chunks — see [Cognitive Design Research](Cognitive-Design-Research.md).

**Each card = one cognitive chunk:**
- Status color dot (preattentive pop-out for instant triage)
- Current task description / micro-summary
- Runtime duration and progress indicator
- Token consumption (input/output breakdown)
- Current operation (reading file / executing command / waiting for approval)
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

### 4. AI Assistant — Diff Summary & Risk Triage (implemented)

A global AI assistant ("butler") that helps developers understand and review what their coding agents have done, without requiring them to read every line of every diff.

**Capabilities (v1):**
- Summarizes diffs across any session, categorizing files by review priority (HIGH: security/config/DB, MEDIUM: business logic/API, LOW: tests/types/formatting)
- Flags specific risks (unparameterized SQL, hardcoded secrets, missing error handling, breaking API changes)
- Answers questions about any session's work, token usage, and status
- Maintains a persistent conversation across app restarts

**Architecture:** Runs as a Tauri sidecar binary (TypeScript compiled with `bun build --compile`), powered by `@mariozechner/pi-ai` (OpenRouter provider) and `@mariozechner/pi-agent-core` (agent runtime with tool calling). Communicates with Rust backend via stdin/stdout JSON lines protocol.

**Tools:** `get_all_sessions` (global awareness), `get_session_diff` (git diff per session), `get_session_costs` (token usage data per session). Tool calls are relayed to Rust for git/SQLite operations.

**Why this replaces the Activity Log:** The original Activity Log aimed to show which files agents read, which commands they ran. The AI assistant provides higher-value intelligence — it doesn't just list changes, it triages them by risk and summarizes what matters. Structured event tracking is deferred; the assistant provides more value than raw event lists.

**Current status:** Fully implemented. Components: `AssistantPanel.tsx`, `AssistantSetup.tsx`, `AssistantChat.tsx`, `AssistantMessage.tsx`. State: `assistantStore.ts`. Backend: `assistant.rs`. Sidecar: `sidecar/` project.

---

## P1: Important, Deferred

These features significantly enhance the product but are not required for initial validation.

| Feature | Description | Dependency |
|---------|-------------|------------|
| **Task Queue & Background Execution** | Queue multiple tasks for sequential execution, or fan out N agents in parallel | Stable session management |
| **Tiered Notification System** | Five-tier alerts (ambient → critical) with anti-fatigue design: deduplication, notification budgets, user thresholds. Audio channel for Tier 3+ per Wickens' Multiple Resource Theory | Session status tracking |
| **Cross-Machine Session Management** | Connect to remote agent sessions via Tailscale, manage from one dashboard | Tailscale integration |
| **Portless Integration** | Auto-assign named URLs per worktree, embed preview window in IDE | Portless setup |
| **Multi-Agent Conflict Detection** | Warn when multiple agents modify the same file | File change tracking |

---

## P2: Future Vision

Lower priority — depends on ecosystem maturity.

| Feature | Description | Blocker |
|---------|-------------|---------|
| **Visual Regression Review** | Screenshot comparison, browser preview | Requires mature agent capabilities |
| **Spec-Driven Development** | Built-in requirements.md / tasks.md editor tied to agent execution | Workflow design needed |
| **Global Knowledge Base** | Cross-session CLAUDE.md management and sync | Multi-session maturity |

[Next: UI Design >](UI-Design.md)
