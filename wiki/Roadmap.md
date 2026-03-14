# Roadmap

[< Home](Home.md) | [< Competitive Analysis](Competitive-Analysis.md)

## Version Plan

### v0.1 — MVP (4-6 weeks)

**Goal:** A working local multi-session manager that solves the top 3 pain points.

| Feature | Detail | Status |
|---------|--------|--------|
| Project scaffold | Tauri 2.x + React 19 + TypeScript 5.8 + Bun | Done |
| Native PTY terminal | xterm.js ↔ `tauri-plugin-pty` real-time bidirectional I/O | Done |
| Multi-session dashboard | Create / stop / switch between sessions with buffer replay | Done |
| Auto worktree + PTY | One-click session creation with auto-provisioning | Done |
| PTY lifecycle management | Spawn, kill, resize, output buffering (1MB/session) | Done |
| Repo-centric session management | SQLite persistence, native folder picker, repo import/remove | Done |
| Session reconciliation | Detect orphaned sessions on startup, mark Disconnected | Done |
| Token usage tracking | Parse Claude Code JSONL files, aggregate token counts (10s poll) | Done |
| Zustand state management | Session store with 11 actions, `useShallow` optimization | Done |
| AI assistant | Diff summary, risk triage, session queries via Pi Agent sidecar | Done |
| File viewer & command palette | Cmd+P fuzzy search, terminal path click, Shiki highlighting overlay | Done |
| Task Board | Kanban board for cognitive offloading + auto agent orchestration (Open→Working→Closed) | Done |
| Git diff viewer | View changes made by agents | **Next** |

**Success criteria:** A developer can manage 3+ concurrent Claude Code sessions from one interface, see token usage per session, and review diffs before accepting changes.

**Recent stabilization work:**
- Fixed xterm.js init race condition (always mount terminal div before init)
- Fixed Zustand infinite re-render loop in CostTracker (`useShallow`)
- Added PTY data flow diagnostic logging

---

### v0.2 — Remote & Isolation (+4 weeks after v0.1)

**Goal:** Support remote machines, proper environment isolation, and session immortality.

| Feature | Detail |
|---------|--------|
| Tailscale remote sessions | Connect to and manage agent sessions on remote machines |
| Session immortality | Agents survive app crashes via remote PTY persistence |
| Docker Sandbox | Opt-in container-based environment isolation |
| Portless naming | Auto-assign URLs per worktree with embedded preview |
| Checkpoint / rollback | Full checkpoint timeline with rollback to any point |
| Multi-agent support | Add Aider and Codex CLI as supported agents |
| Visual diff review | Side-by-side diff view with per-file accept/reject |

---

### v0.3 — Orchestration & SDK (+4 weeks after v0.2)

**Goal:** Advanced multi-agent workflows and deeper integration.

| Feature | Detail |
|---------|--------|
| Task queue enhancements | Task dependencies, priority ordering, bulk operations, drag-and-drop |
| Parallel orchestration | Fan out N agents working in parallel from Task Board |
| Conflict detection | Warn when multiple agents touch the same files |
| Agent SDK integration | Direct Claude Code SDK integration (structured output) |
| Multi-model backends | Support for alternative model providers |

---

## Design Adjustments from Original Research

The original research document's framework was sound, but this roadmap makes five key adjustments:

1. **Cut over-engineering:** Global memory browser, visual regression engine, and policy conflict bus are deferred to P2 vision. MVP focuses on validated high-frequency pain points only.

2. **Skipped tmux in favor of native PTY:** The original Phase 1 (tmux send-keys) was bypassed. Direct PTY bridging via `tauri-plugin-pty` provides real-time rendering without polling overhead. Tradeoff: sessions don't survive app crashes (addressed in v0.2).

3. **Single-process architecture:** The original daemon + WebSocket design was simplified to a single Tauri process with IPC. Reduces complexity for local-first MVP.

4. **Emphasized agent-agnosticism:** This is the competitive moat — the original doc didn't emphasize it enough.

5. **Defined clear MVP scope:** A deliverable product in 4-6 weeks, not a grand vision document.

---

## Milestones Timeline

```
March 2026                          April 2026                        May 2026
|--- v0.1 MVP Development ---------|--- v0.2 Remote & Isolation -----|--- v0.3 Orchestration ------>
     |                                   |                                 |
     Dashboard + Terminal + Cost         Tailscale + Docker + Rollback     Task Queue + SDK
```
