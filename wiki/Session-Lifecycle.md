# Session Lifecycle

[< Home](Home.md) | [< Technical Architecture](Technical-Architecture.md)

Racc stores session metadata in SQLite and owns session transports in `racc-core`. The frontend requests lifecycle operations; it does not spawn or kill PTYs itself.

## Repository Import

```text
Choose repository
    -> validate git repository and duplicate path
    -> insert repo metadata in ~/.racc/racc.db
    -> show repository in sidebar
```

In desktop mode the path normally comes from a native picker. Browser mode sends the server-side filesystem path through the WebSocket command.

## Normal Session Creation

Sessions can be created directly or by firing an Open task.

```text
Choose agent/location/options
    -> optionally create git worktree
    -> persist session row as Running
    -> best-effort agent setup (for example RTK for Claude Code)
    -> create LocalPtyTransport or SshTmuxTransport
    -> launch the selected agent
    -> wait for the agent's ready prompt
    -> send task description and image paths
    -> stream output through the shared terminal channel
```

Current selectable agents are Claude Code and Codex. The permission-bypass choice is persisted so reattach can preserve the original intent.

### Working Directory

- Without a worktree, the session runs in the imported repository.
- With a worktree, Racc creates a branch under `~/racc-worktrees/{repo}/...` and starts there.
- Remote sessions use the corresponding path on the SSH host and run inside tmux.

## State Model

```text
                 create / reattach
                        |
                        v
                    Running
                  /    |     \
          user stop  failure  transport lost
              |        |           |
              v        v           v
          Completed   Error    Disconnected
              \        |           /
               \-------+----------/
                       |
                 reopen / retry
```

| State | Meaning | Typical actions |
|-------|---------|-----------------|
| **Running** | Backend transport is active or a remote tmux session was confirmed live | Open terminal, send input, stop, remove |
| **Completed** | User stopped the session or the remote tmux process no longer exists | Reattach, inspect metadata, remove |
| **Disconnected** | Metadata survives but the transport or host is unavailable | Reconnect/reattach, remove |
| **Error** | Creation or an operation failed | Inspect error, retry, remove |

Waiting/Paused may be surfaced by agent-output interpretation, but transport liveness remains the core lifecycle distinction.

## Opening and Reconnecting

Selecting a session calls `reconnect_session` before the terminal is shown.

- If its transport is already registered and alive, Racc returns without duplicating it.
- A live remote tmux session is reconnected through SSH and its buffered output resumes.
- A session needing a fresh process follows full reattach.

### Agent Resume

- New Claude Code sessions record their conversation UUID. Reattach uses that exact UUID; legacy rows fall back to Claude Code's directory-based continuation.
- Codex reattach uses the Codex resume flow for the latest conversation in that working directory.
- Terminal scrollback from a dead local PTY is not a durable transcript. The resumed agent's conversation context and Racc's session metadata are the durable pieces.

## Startup Reconciliation

At application/server startup, `reconcile_sessions()` audits rows previously marked Running.

### Local sessions

Local PTYs belong to the old Racc process and cannot survive it. They become Disconnected.

### Remote sessions

Racc reconnects to the configured SSH host and probes the session's tmux name:

- tmux exists: keep Running and restore the transport;
- tmux is gone: mark Completed;
- host is unreachable or the probe cannot be completed: mark Disconnected.

This is why remote sessions can survive a UI/app restart while local sessions require reattach.

## Stop, Remove, and Repository Cleanup

- **Stop** terminates the registered local PTY or remote tmux transport and marks the row Completed.
- **Remove session** removes its database record. If it owns a worktree, the confirmation UI can also request `git worktree remove`.
- **Remove repository** is blocked while it has Running sessions and cascades associated persisted records when permitted.
- Browser disconnect alone does not stop server-owned transports; reconnecting the browser receives current metadata and terminal data from the server.

## Planner and Manager Sessions

Task Planner, Merge Manager, and Test Manager create specialized sessions with generated prompts. Each run also starts a loopback-only MCP endpoint carrying a random bearer capability in the agent environment.

The endpoint accepts only the tool and run/repository identity for that run. On valid submission, `racc-core` stores the structured result and emits the corresponding UI event.

If a manager transport ends or its endpoint stops before a result is accepted:

- Merge Manager and Test Manager move to `needs_review` so external merge/test state is not guessed.
- Restarting the old manager session cannot restore the expired capability; the user must verify and resolve the run or start Retry.
- Task Planner reports failure rather than creating tasks from terminal text.

## Terminal Data and Buffering

Both transports publish bytes to `AppContext.terminal_tx`. A bounded ring buffer is maintained per session by `TransportManager`.

- Tauri mode forwards terminal data as native events.
- Headless mode sends binary WebSocket frames: 8-byte little-endian session ID followed by terminal bytes.
- Client input uses the same binary frame shape in browser mode, while resize and buffer requests are JSON calls.

[Next: WebSocket Remote API >](WebSocket-Remote-API.md)
