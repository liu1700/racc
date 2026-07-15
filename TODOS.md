# TODOs and Known Gaps

This list tracks current deferred work. Historical implementation checklists live in the labelled design-record pages under `wiki/`.

## Reliability

### Remove the initial shell-launch delay

Task text is already sent only after Racc detects that the agent is ready for input. The remaining fixed delay is the short pause before Racc writes the initial agent command into a newly created shell PTY. Replace that delay with explicit shell-readiness detection if slow-host launch failures appear.

### Durable manager result endpoints

Task Planner, Merge Manager, and Test Manager use capability-scoped loopback MCP endpoints tied to one run. If the manager session is restarted after that endpoint expires, Racc correctly marks the run `needs_review`, but cannot resume structured submission in the restarted terminal. A future design could safely reissue a scoped endpoint without widening its authority.

### Broader automated UI coverage

Core workflows have Rust tests and selected frontend unit tests, but the complete Tauri UI does not yet have a cross-platform end-to-end harness. Add repeatable coverage for task firing, terminal links, planner confirmation, merge queues, test runs, and restart recovery.

## Product

### Notification policy

Racc can emit native notifications for selected events, but the broader supervisor notification model remains incomplete. Add configurable thresholds, aggregation, and optional webhooks without creating alert fatigue.

### Task dependency execution

The Task Planner can generate dependency metadata and enforces prerequisites while selecting a preview. The normal task board does not yet schedule or automatically execute a dependency graph.

### Multi-agent cost coverage

The status bar reads Claude Code usage data. Equivalent per-session and per-task usage reporting for Codex is not yet available.

### Setup and distribution

Improve signed installers, one-command setup, and first-run guidance across macOS, Linux, Windows, and remote hosts.

### Optional sandboxing

Git worktrees isolate source changes, not operating-system permissions. A future opt-in container sandbox should make high-autonomy sessions safer without becoming mandatory for lightweight workflows.

## Security

### Headless authentication and TLS

`racc-server` binds to `0.0.0.0` and currently relies on a trusted private network such as Tailscale. Add application-level authentication, origin controls, and a supported TLS deployment path before recommending public exposure.
