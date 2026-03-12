# Product Vision

[< Home](./Home.md)

## Core Positioning

OTTE is a **control plane for AI coding agents** — a desktop app that manages multiple agentic coding sessions with visibility, cost tracking, and change review.

It is explicitly **not**:
- A code editor (VS Code, Neovim already exist)
- A new AI agent (Claude Code, Aider already exist)
- A replacement for git, tmux, or docker (it orchestrates them)

## Target User

**Primary persona:** An individual developer who:

- Uses Claude Code / Aider daily as their main coding workflow
- Maintains multiple projects or feature branches simultaneously
- Currently uses tmux + git worktrees to run agents in parallel
- Struggles with lack of visual overview, cost tracking, and change review
- May work across local machines and remote VPS instances

**Key insight:** These users love the power of terminal agents but need a management layer on top. They don't want to give up terminal agents for Cursor/Windsurf — they want to augment them.

## Design Principles

### 1. Agent-Agnostic

The IDE must work with **any** terminal-based coding agent. This is the core differentiator. When AI models evolve rapidly, not being locked to a single vendor is the highest user value.

Implementation: A unified **Agent Adapter** interface that abstracts communication. MVP uses tmux send-keys (works with everything), later adds PTY bridging and SDK integration for specific agents.

### 2. Session Immortality

Agent sessions must survive:
- IDE crashes
- Network disconnects
- Laptop lid closes
- Machine reboots (on remote VPS)

Implementation: Every session runs inside a tmux session. The IDE is a viewer, not the host. Disconnect and reconnect seamlessly.

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
- **tmux** for session persistence (not custom process management)
- **docker** for environment isolation (not Nix or Firecracker)
- **Tailscale** for networking (not custom VPN)

[Next: Feature Specification >](./Feature-Specification.md)
