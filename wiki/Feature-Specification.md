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
- Token consumption & estimated cost
- Current operation (reading file / executing command / waiting for approval)
- Associated git branch and worktree path

**Categorical grouping:** Sessions are ordered by status priority (error/blocked → running → completed) so "needs attention" items always surface first. When 10 agents are grouped into 3 status categories, the developer holds 3 chunks rather than 10.

**Key actions:**
- One-click new session creation (auto-creates worktree + spawns PTY + starts agent)
- Stop / terminate sessions
- Quick switch between sessions (with PTY output buffer replay)

### 2. Real-Time Cost Tracking

This is the **#1 user pain point** — the community has independently built 7+ monitoring tools, proving urgency.

**Per-session:**
- Token consumption (input/output breakdown)
- Estimated cost in real-time on the session card

**Global (status bar):**
- Total spend across all sessions
- Weekly/monthly spend
- Ratio against subscription quota
- Configurable alert thresholds

**MVP approach:** Read Claude Code's local usage data files. Support for other agents' cost data in later versions.

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

### 4. Agent Activity Transparency Log *(not yet implemented)*

Structured view of every agent operation.

**Logged events:**
- Files read (with paths)
- Search queries executed
- Shell commands run
- Decisions made
- Tool calls and their results

**Features:**
- Filterable by event type
- Full-text searchable
- Timestamp-ordered timeline

**Why this matters:** Current agents show "Read 3 files" — but which 3? This solves the transparency problem.

**Current status:** UI placeholder exists in `ActivityLog.tsx`. Structured event parsing is planned for P1.

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
