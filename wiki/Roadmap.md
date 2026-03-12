# Roadmap

[< Home](./Home.md) | [< Competitive Analysis](./Competitive-Analysis.md)

## Version Plan

### v0.1 — MVP (4-6 weeks)

**Goal:** A working local multi-session manager that solves the top 3 pain points.

| Feature | Detail | Status |
|---------|--------|--------|
| Project scaffold | Tauri 2.x + React + TypeScript + Bun | Done (#2) |
| Multi-session dashboard | Create / stop / switch between sessions | Scaffolded |
| Auto worktree + tmux | One-click session creation with auto-provisioning | Scaffolded |
| PTY terminal rendering | xterm.js rendering of agent sessions | **Next** |
| tmux send-keys injection | Send prompts to agents via tmux | Scaffolded |
| Basic cost tracking | Read Claude Code's local usage data | Scaffolded |
| Git diff viewer | View changes made by agents | Planned |

**Success criteria:** A developer can manage 3+ concurrent Claude Code sessions from one interface, see cost per session, and review diffs before accepting changes.

---

### v0.2 — Remote & Isolation (+4 weeks after v0.1)

**Goal:** Support remote machines and provide proper environment isolation.

| Feature | Detail |
|---------|--------|
| Tailscale remote sessions | Connect to and manage agent sessions on remote machines |
| Docker Sandbox | Opt-in container-based environment isolation |
| Portless naming | Auto-assign URLs per worktree with embedded preview |
| Checkpoint / rollback | Full checkpoint timeline with rollback to any point |
| Multi-agent support | Add Aider and Codex CLI as supported agents |

---

### v0.3 — Orchestration & SDK (+4 weeks after v0.2)

**Goal:** Advanced multi-agent workflows and deeper integration.

| Feature | Detail |
|---------|--------|
| Task queue | Queue tasks for sequential agent execution |
| Parallel orchestration | Fan out N agents working in parallel |
| Conflict detection | Warn when multiple agents touch the same files |
| Agent SDK integration | Direct Claude Code SDK integration (structured output) |
| Multi-model backends | Support for alternative model providers |

---

## Design Adjustments from Original Research

The original research document's framework was sound, but this roadmap makes four key adjustments:

1. **Cut over-engineering:** Global memory browser, visual regression engine, and policy conflict bus are deferred to P2 vision. MVP focuses on validated high-frequency pain points only.

2. **Added implementation phasing:** Each technical choice has a short/mid/long-term strategy rather than a one-shot design.

3. **Emphasized agent-agnosticism:** This is the competitive moat — the original doc didn't emphasize it enough.

4. **Defined clear MVP scope:** A deliverable product in 4-6 weeks, not a grand vision document.

---

## Milestones Timeline

```
March 2026                          April 2026                        May 2026
|--- v0.1 MVP Development ---------|--- v0.2 Remote & Isolation -----|--- v0.3 Orchestration ------>
     |                                   |                                 |
     Dashboard + Terminal + Cost         Tailscale + Docker + Rollback     Task Queue + SDK
```
