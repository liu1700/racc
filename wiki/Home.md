# Racc Documentation

> Racc is a desktop and browser control plane for Claude Code and Codex sessions.

Racc combines a task board, isolated git worktrees, native terminals, remote SSH/tmux sessions, and dedicated planning, merge, and test workflows. It is not a code editor; it gives terminal agents a shared operational surface.

## Start Here

| Page | Description |
|------|-------------|
| [User Guide](User-Guide.md) | Current product workflow, managers, terminal links, and recovery behavior |
| [Feature Specification](Feature-Specification.md) | Implemented capabilities and known boundaries |
| [UI Design](UI-Design.md) | Current layout, task columns, terminal, and interaction patterns |
| [Session Lifecycle](Session-Lifecycle.md) | Local and remote creation, reconnect, resume, and cleanup semantics |
| [Technical Architecture](Technical-Architecture.md) | Three-crate backend, transports, data flow, workflow MCP, and persistence |
| [WebSocket Remote API](WebSocket-Remote-API.md) | Headless `/ws` protocol, methods, events, and terminal frames |
| [RTK Token Optimization](RTK-Token-Optimization.md) | Automatic output compression for Claude Code sessions |
| [Roadmap](Roadmap.md) | Completed foundations, current gaps, and next directions |

## Product and Research

| Page | Status |
|------|--------|
| [Product Vision](Product-Vision.md) | Current positioning and design principles |
| [Cognitive Design Research](Cognitive-Design-Research.md) | Research background informing the interface |
| [Agent Supervisor Design](Agent-Supervisor-Design.md) | Design record; not a description of fully implemented autonomous scheduling |
| [Headless Server Design](Headless-Server-Design.md) | Architecture record for the implemented core/server extraction |
| [Headless Server Plan](Headless-Server-Plan.md) | Historical implementation plan; retained for provenance |

## Current Technical Baseline

- **Frontend:** React 19, TypeScript, Zustand, Tailwind, and xterm.js.
- **Desktop:** Tauri 2.x with thin IPC wrappers.
- **Shared backend:** `racc-core` owns SQLite, git/session commands, local PTY, SSH/tmux, and workflow managers.
- **Browser mode:** `racc-server` serves the same frontend and exposes `/ws` through Axum.
- **Agents:** Claude Code and Codex are selectable for normal tasks, planning, merging, and testing.
- **Structured workflows:** Task Planner, Merge Manager, and Test Manager update Racc through run-scoped MCP tools rather than terminal JSON sentinels.
- **Persistence:** Metadata lives in SQLite at `~/.racc/racc.db`; source isolation uses git worktrees.

## Design Principles

1. Do not rebuild a code editor.
2. Keep the terminal-agent layer vendor-flexible.
3. Integrate with git, SSH, tmux, and native OS behavior.
4. Make parallel work easy to triage and safe to recover.
