# OTTE — Agentic IDE

> A desktop control plane for orchestrating multiple AI coding agents.
> Not an editor. Not an IDE. A **session orchestrator**.

## What is OTTE?

OTTE is a standalone desktop application (Tauri) for individual developers who use terminal-based AI coding agents (Claude Code, Aider, Codex CLI, etc.). It provides visual management for multiple concurrent agent sessions — something the terminal alone cannot offer.

**Three design principles — the "Three Nots":**

1. **Don't rebuild a code editor** — users already have one they love
2. **Don't lock into a specific agent** — Claude Code, Aider, Codex should all work
3. **Don't replace existing tools** — integrate with git, tmux, docker instead

## Quick Navigation

| Page | Description |
|------|-------------|
| [Product Vision](./Product-Vision.md) | Core positioning, target users, design principles |
| [Feature Specification](./Feature-Specification.md) | P0 (MVP), P1, and P2 features in detail |
| [UI Design](./UI-Design.md) | Layout, panels, and interaction patterns |
| [Technical Architecture](./Technical-Architecture.md) | System architecture, tech stack, and tradeoffs |
| [Session Lifecycle](./Session-Lifecycle.md) | State machine, creation flow, reconnection |
| [Competitive Analysis](./Competitive-Analysis.md) | How OTTE differs from Cursor, Windsurf, Claude Squad |
| [Roadmap](./Roadmap.md) | MVP scope and versioned milestones |

## Key Technical Bets

- **Tauri 2.x** — Rust backend + React frontend + xterm.js terminals
- **tmux** — Session persistence layer (agents survive disconnects)
- **PTY bridging** — Agent-agnostic communication via terminal I/O
- **Git worktrees** — Code isolation per session, zero overhead
- **Tailscale** — Cross-machine mesh networking for remote sessions

## One-Line Summary

> OTTE is "the next step for terminal agent users" — keep the full power of their favorite agents, add the orchestration, review, and visibility they've always lacked.
