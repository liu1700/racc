# Feature Specification

[< Home](Home.md) | [< Product Vision](Product-Vision.md)

This page describes the implemented product baseline. Future work is tracked separately in [Roadmap](Roadmap.md) and [TODOs](../TODOS.md).

## Repository and Session Management

- Import local git repositories and organize sessions underneath each repository.
- Start Claude Code or Codex in the repository or an automatically created git worktree.
- Optionally bypass agent permission prompts; the choice is persisted for later reattach.
- Run locally through a native PTY or on a configured server through SSH/tmux.
- Stop, remove, reconnect, or reattach sessions and optionally remove their worktrees.
- Store a detected GitHub pull-request URL on the session for direct access and merge queuing.
- Show status, branch, elapsed time, and recent terminal output in the sidebar.

## Task Board

The Tasks view uses four equal-width columns:

### Open

- Create and edit multiline tasks.
- Paste or select image attachments.
- Generate a task preview with Task Planner.
- Fire a task with agent, permissions, worktree, branch, and server choices.

### Working

- Associate each task with its running session.
- Show live activity, branch, elapsed time, images, and PR state.
- Mark a task's exact GitHub PR URL **Ready to merge**.
- Automatically archive a task when its associated session completes or is removed.

Archived tasks retain the database status `closed`, but there is no visible Closed column.

### Merge Manager

- Keep an ordered per-repository PR queue.
- Save target branch, Claude Code/Codex choice, and editable instructions per repository.
- Start one integration session and worktree for the queue.
- Require validation of the combined tree, then merge eligible PRs in order.
- Record per-item outcomes and a typed overall result.
- Surface `needs_review` when completion cannot be proven, with **Mark succeeded**, **Mark failed**, and **Retry** actions.

Merge Manager stops before later remote merges when an earlier queue item conflicts or the combined validation batch fails.

### Test Manager

- Save target branch, Claude Code/Codex choice, and editable test instructions per repository.
- Default to a comprehensive full-project automated and end-to-end UAT pass.
- Start from the **Run** button in a dedicated `racc/test-*` worktree.
- Instruct the agent not to modify product code, weaken tests, commit, push, or merge.
- Store every command/UAT scenario, status, evidence, and summary.
- Show passed/failed totals and the same explicit recovery actions as Merge Manager.

## Task Planner

- Accept an Epic URL or pasted product/feature description.
- Run Claude Code or Codex read-only against the selected repository.
- Return a source-faithful task preview with stable keys and dependency metadata.
- Select nothing by default; dependent selections automatically include prerequisites.
- Create only the tasks explicitly confirmed by the user.

If an external source requires credentials unavailable to the agent, the UI directs the user to paste its text.

## Structured Workflow Reporting

Task Planner, Merge Manager, and Test Manager each receive a loopback-only, run-scoped MCP endpoint and bearer capability at launch. Their completion tools are:

- `racc_task_plan.submit_task_plan`
- `racc_merge_manager.submit_merge_result`
- `racc_test_manager.submit_test_result`

The core validates the submitted run/repository identity, stores the structured result in SQLite, and emits an event that refreshes desktop and browser UIs. Terminal text and printed JSON are not parsed as workflow completion signals.

## Terminal and File Access

- Full xterm.js terminal with resize, input, and backend buffer replay.
- HTTP(S) links open through the Tauri shell plugin on desktop or a new browser tab in headless mode.
- Only HTTP(S) schemes are accepted for external opening.
- Repository file paths detected in terminal output open in the built-in viewer, optionally at a line number.
- `Cmd+P` opens fuzzy repository file search; `Cmd+F` and `Ctrl+G` work inside the viewer.
- Shiki provides syntax highlighting.

## Remote Servers and Headless Mode

- Add servers manually or import SSH config hosts.
- Test, connect, disconnect, edit, remove, and run remote setup.
- Remote sessions use tmux so they can survive a UI restart.
- `racc-server` serves the production React build and exposes `/ws` on port `9399` by default.
- Browser and desktop modes use the same core commands and event model.

Headless mode currently binds to all interfaces without application-level authentication or TLS. It is intended for a trusted private network, not public internet exposure.

## Usage and Output Optimization

- Display global and per-project Claude Code token usage from local usage data.
- Automatically install/configure RTK for Claude Code when possible.
- Treat RTK setup as best-effort; session creation continues if optimization setup fails.
- Codex usage accounting and RTK rewriting are not currently provided.

## Persistence

SQLite schema version 6 stores repositories, sessions, tasks, servers, event/insight records, task-plan runs, merge settings/runs/queue items, and test settings/runs. Task images live under each repository's `.racc/images/` directory.

[Next: UI Design >](UI-Design.md)
