# Product Vision

[< Home](Home.md)

## Core Positioning

Racc is a **control plane for terminal-based AI coding agents**. It helps one developer turn a set of repository tasks into isolated agent sessions, understand what is happening across those sessions, and move completed work through testing and merging.

It is explicitly not:

- a code editor;
- a new foundation model or coding agent;
- a replacement for git, SSH, tmux, or a project's own test system;
- a promise that high-autonomy agents are safe without review.

## Target User

The primary user already works with Claude Code or Codex in terminals and wants to run several pieces of work at once. They may work locally, on a remote development machine, or from a browser over a trusted private network.

Their main problem is no longer starting an agent. It is maintaining a reliable mental model of parallel work: which task is open, which session needs attention, which PRs are ready, what was actually tested, and whether an interrupted automation really finished.

## Product Loop

```text
Describe work
    -> review tasks
    -> fire isolated sessions
    -> monitor terminals
    -> queue PRs
    -> validate and merge
    -> run full-project UAT
```

Task Planner, Merge Manager, and Test Manager turn the beginning and end of this loop into explicit, inspectable workflows. The terminal remains available throughout; automation never removes the ability to see what the agent did.

## Design Principles

### 1. Agent-Agnostic Transport

Agent communication uses normal terminal input and output. Claude Code and Codex are supported today, while the transport boundary remains suitable for other terminal agents.

Agent-specific behavior is kept narrow: launch/resume arguments, usage data, and optional setup such as RTK for Claude Code.

### 2. Isolation by Default

Normal parallel tasks use git worktrees so source changes do not collide. Merge Manager and Test Manager create their own integration/test worktrees. A worktree isolates git state, not machine permissions; stronger container isolation remains future work.

### 3. Resilient, Honest State

SQLite preserves repositories, tasks, sessions, manager settings, runs, and results. Local PTYs are process-bound and become Disconnected after restart; remote tmux sessions can be probed and reconnected.

Racc must not guess that a critical workflow succeeded. Manager agents submit typed results through run-scoped MCP tools. If the endpoint or session disappears before submission, Racc surfaces `needs_review` and gives the user explicit resolution and retry actions.

### 4. Transparency Over Magic

Every workflow retains an inspectable terminal. Structured summaries supplement the underlying commands and evidence; they do not conceal them. External links, source files, branches, worktrees, PRs, and test results remain directly reachable from the UI.

### 5. Design for Human Attention

The board separates intent (Open), execution (Working), release integration (Merge Manager), and verification (Test Manager). The sidebar compresses each live session into status, elapsed time, branch, and recent output. Completed tasks are archived rather than kept as a permanently visible column.

See [Cognitive Design Research](Cognitive-Design-Research.md) for the research background.

### 6. Shared Core, Multiple Surfaces

Desktop and browser modes use the same `racc-core` behavior. Tauri IPC and WebSocket are transport choices, not separate products. This keeps task, session, merge, and test semantics consistent across devices.

### 7. Integrate Instead of Rebuild

Racc deliberately relies on:

- git worktrees for source isolation;
- native PTY for local sessions;
- SSH/tmux for persistent remote sessions;
- SQLite for local metadata;
- Tailscale or an equivalent trusted network for private headless access;
- each repository's own build and test commands.

[Next: Feature Specification >](Feature-Specification.md)
