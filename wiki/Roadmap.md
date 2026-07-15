# Roadmap

[< Home](Home.md)

This roadmap reflects the current codebase rather than the original calendar estimates. Completed implementation plans remain available as design records.

## Implemented Foundation

| Area | Current state |
|------|---------------|
| Desktop application | Tauri 2.x app with React 19 shared frontend |
| Shared backend | Three-crate workspace with `racc-core`, `racc-server`, and thin Tauri wrappers |
| Local sessions | Native PTY, xterm.js terminal, backend buffering, worktree creation, reattach |
| Remote sessions | SSH server management and persistent tmux transports |
| Browser access | Axum static server plus full-duplex `/ws` command/event/terminal transport |
| Supported agents | Claude Code and Codex across tasks, planning, merging, and testing |
| Task workflow | Open and Working task management with images and archived completion state |
| Task Planner | Read-only AI plan generation, dependency-aware preview, selective confirmation |
| Merge Manager | Ordered PR queue, combined validation, structured results, recovery states |
| Test Manager | Isolated full-project UAT runs with editable defaults and structured evidence |
| File access | Fuzzy command palette, syntax-highlighted viewer, clickable terminal paths |
| External links | Direct HTTP(S) opening from terminal and PR surfaces |
| Usage optimization | Claude Code usage display and best-effort RTK setup |

## Near-Term Priorities

### Reliability and Verification

- Add repeatable desktop/headless end-to-end coverage for task, planner, merge, test, and restart flows.
- Replace the remaining fixed shell-launch pause with readiness detection if real-world hosts require it.
- Design a safe way to recover or reissue expired run-scoped manager MCP endpoints.
- Improve diagnostics for missing CLIs, build prerequisites, SSH failures, and local service conflicts.

### Security and Distribution

- Add authentication, origin controls, and a documented TLS path for `racc-server`.
- Improve signed release artifacts and platform-specific installation guidance.
- Add a first-run setup path for local and remote agent prerequisites.

### Workflow Quality

- Add optional scheduling based on task dependencies and priorities.
- Expand notification controls and optional webhook delivery.
- Add Codex usage/cost visibility comparable to Claude Code reporting.
- Improve history and audit views for archived tasks and past manager runs.

## Longer-Term Directions

- Opt-in container isolation for high-autonomy sessions.
- Pluggable adapters for additional terminal agents.
- Reusable workflow templates beyond planning, merging, and UAT.
- Better cross-device coordination and multi-user security for headless deployments.
- Evidence-rich release gates that connect task intent, PR changes, automated tests, and UAT results.

## Explicitly Not Promised Yet

- Public-internet-safe headless hosting without additional network controls
- Fully autonomous conflict resolution or silent merge recovery
- Operating-system isolation from git worktrees alone
- Automatic execution of Task Planner dependency graphs
- Identical usage accounting across agent vendors

[Next: User Guide >](User-Guide.md)
