# Product Vision

[< Home](Home.md)

## Core Positioning

OTTE is a **control plane for AI coding agents** — a desktop app that manages multiple agentic coding sessions with visibility, cost tracking, and change review.

It is explicitly **not**:
- A code editor (VS Code, Neovim already exist)
- A new AI agent (Claude Code, Aider already exist)
- A replacement for git or docker (it orchestrates them)

## Target User

**Primary persona:** An individual developer who:

- Uses Claude Code / Aider daily as their main coding workflow
- Maintains multiple projects or feature branches simultaneously
- Currently runs agents in multiple terminals or tmux panes to work in parallel
- Struggles with lack of visual overview, cost tracking, and change review
- May work across local machines and remote VPS instances

**Key insight:** These users love the power of terminal agents but need a management layer on top. They don't want to give up terminal agents for Cursor/Windsurf — they want to augment them.

## Design Principles

### 1. Agent-Agnostic

The IDE must work with **any** terminal-based coding agent. This is the core differentiator. When AI models evolve rapidly, not being locked to a single vendor is the highest user value.

Implementation: A unified **Agent Adapter** interface that abstracts communication. MVP uses tmux send-keys (works with everything), later adds PTY bridging and SDK integration for specific agents.

### 2. Session Resilience

Agent sessions are managed with graceful handling of disruptions:
- On app close, all PTY processes are cleaned up
- On app restart, `reconcile_sessions()` marks orphaned sessions as `Disconnected`
- SQLite persistence ensures session metadata survives across restarts

Implementation: Each session runs in a native PTY process managed by `tauri-plugin-pty`. Session metadata is persisted in SQLite. On restart, reconciliation detects orphaned sessions and updates their status. Full session immortality (surviving app crashes) is planned for v0.2 via remote execution support.

### 3. Transparency Over Magic

Users must be able to see exactly what their agents are doing:
- Which files were read
- Which commands were executed
- How much money was spent
- What changes were made

No black boxes. Every agent action is logged, searchable, and filterable.

### 4. Integration Over Reinvention

Build on battle-tested tools:
- **git worktrees** for code isolation (not custom sandboxing)
- **native PTY** for agent communication (standard terminal I/O)
- **SQLite** for session persistence (lightweight, embedded)
- **docker** for environment isolation (not Nix or Firecracker) — planned v0.2
- **Tailscale** for networking (not custom VPN) — planned v0.2

[Next: Feature Specification >](Feature-Specification.md)
