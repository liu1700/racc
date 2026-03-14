# WebSocket Remote API for Racc

**Date**: 2026-03-14
**Status**: Approved

## Summary

Expose a WebSocket server from Racc's Rust backend, allowing external agent clients (e.g., OpenClaw) and other devices to remotely create tasks, start sessions, and receive status updates — without accessing the Racc UI.

## Requirements

- **Scope**: Task CRUD + Session control (create/stop/reattach) + read-only queries (repos, diffs)
- **Protocol**: WebSocket (JSON text frames)
- **Auth**: None for now (future addition)
- **Output**: Status change events only (no terminal output streaming)
- **Lifecycle**: Auto-start with Racc, default port `9399`, bind `127.0.0.1` (localhost only; `0.0.0.0` opt-in via future settings)

## Architecture

### WebSocket Server

Embedded in the Tauri Rust backend using `tokio-tungstenite`. Spawned as a tokio task in `setup()`, holding a clone of `AppHandle` to access managed state.

```
External Client ──WebSocket──▶ ws_server.rs ──▶ Existing command logic
                                                    │
                                              SQLite DB (shared)
                                                    │
                                              broadcast channel
                                                    │
                                    ┌───────────────┼───────────────┐
                                    ▼                               ▼
                              WebSocket broadcast            AppHandle.emit()
                              (to all clients)               (to frontend)
```

### Connection Management

- `Arc<RwLock<HashMap<u64, SplitSink<WebSocketStream<TcpStream>, Message>>>>` — active connection pool (type-aliased as `WsSink`)
- Auto-incrementing connection IDs
- Automatic cleanup on disconnect
- Ping/pong heartbeat: server sends ping every 30s, disconnects clients that miss 2 consecutive pongs

### Event Bus

A `tokio::broadcast` channel (`broadcast::Sender<RaccEvent>`, capacity: 64) added to Tauri managed state. All session/task state mutations (from Tauri commands or WebSocket handlers) send events to this channel after DB writes.

If a WebSocket client falls behind (receives `RecvError::Lagged`), the missed events are silently dropped and the client continues from the latest event.

Consumers:
- WebSocket connection manager: broadcasts to all connected clients
- Frontend: receives via `AppHandle.emit()` for UI sync

### DB Access from WebSocket Handlers

The existing `std::sync::Mutex<Connection>` must not be held across `.await` points. All DB operations in WebSocket handlers run inside `tokio::task::spawn_blocking()`, acquiring and releasing the mutex within the blocking closure. This matches how Tauri command handlers work internally.

## Message Protocol

### Client → Racc (Request)

```json
{
  "id": "req_1",
  "method": "create_task",
  "params": { "repo_id": 1, "description": "Fix login bug" }
}
```

`id` is a client-chosen string for correlating responses. The server does not enforce uniqueness — it echoes the `id` back verbatim.

### Racc → Client (Response)

```json
{
  "id": "req_1",
  "result": { "task_id": 5 }
}
```

### Racc → Client (Error)

```json
{
  "id": "req_1",
  "error": "Repo not found"
}
```

Errors are opaque strings. No error codes for now.

### Racc → Client (Push Event, no id)

```json
{
  "event": "session_status_changed",
  "data": { "session_id": 3, "status": "completed", "pr_url": "https://..." }
}
```

## Exposed Methods

### Task Operations

| Method | Params | Returns |
|--------|--------|---------|
| `create_task` | `repo_id`, `description` | `{ task_id }` |
| `list_tasks` | `repo_id` | `{ tasks: [...] }` |
| `update_task_status` | `task_id`, `status`, `session_id?` | `{}` |
| `update_task_description` | `task_id`, `description` | `{}` |
| `delete_task` | `task_id` | `{}` |

### Session Operations

| Method | Params | Returns |
|--------|--------|---------|
| `create_session` | `repo_id`, `use_worktree`, `branch`, `agent?` | `{ session_id }` |
| `stop_session` | `session_id` | `{}` |
| `reattach_session` | `session_id` | `{ session }` |

- `create_session`: `agent` defaults to `"claude-code"`. Remote sessions always run with `--dangerously-skip-permissions` (same as current UI behavior).
- `stop_session`: Updates DB status to "Completed" AND emits a Tauri event so the frontend kills the PTY (see PTY Lifecycle section).
- `reattach_session`: Returns the full `Session` object.

### Query Operations

| Method | Params | Returns |
|--------|--------|---------|
| `list_repos` | — | `{ repos: [...] }` |
| `get_session_diff` | `session_id` | `{ diff: "..." }` |

- `get_session_diff`: Looks up the session's `worktree_path` from DB. If the session has no worktree, falls back to the repo's root path. Returns an error if neither path exists.

### Push Events (Server → Client)

| Event | Data |
|-------|------|
| `session_status_changed` | `{ session_id, status, pr_url? }` |
| `task_status_changed` | `{ task_id, status, session_id? }` |
| `task_deleted` | `{ task_id }` |

**Not exposed** (destructive): `remove_session`, `remove_repo`, `reset_db`.

## RaccEvent Enum

```rust
#[derive(Clone, Debug, serde::Serialize)]
enum RaccEvent {
    SessionStatusChanged { session_id: i64, status: String, pr_url: Option<String> },
    TaskStatusChanged { task_id: i64, status: String, session_id: Option<i64> },
    TaskDeleted { task_id: i64 },
}
```

## PTY Lifecycle Coordination

PTY spawning and killing is frontend-only (`ptyManager.ts`). The frontend must listen for Tauri events to coordinate with remote commands.

### Tauri Event Contract

Two new Tauri events emitted via `AppHandle.emit()`:

| Event Name | Payload | Frontend Action |
|------------|---------|-----------------|
| `racc://session-created` | `{ session_id, repo_id, branch, worktree_path, agent, source: "remote" }` | Call `ptyManager.spawnPty()` to start agent |
| `racc://session-stopped` | `{ session_id, source: "remote" }` | Call `ptyManager.killPty()` to terminate PTY |

The `source` field distinguishes local UI actions (`"local"`) from remote WebSocket commands (`"remote"`). The frontend only reacts to `"remote"` events to avoid double-processing its own actions.

### Frontend Listener Setup

Add to `sessionStore.ts` `initialize()`:

```typescript
import { listen } from '@tauri-apps/api/event';

// In initialize():
listen('racc://session-created', async (event) => {
  const { session_id, repo_id, worktree_path, agent, source } = event.payload;
  if (source !== 'remote') return;
  // Refresh session list from DB
  await get().loadRepos();
  // Spawn PTY for the remotely-created session
  const agentCmd = `claude --dangerously-skip-permissions`;
  await spawnPty(session_id, worktree_path, 80, 24, agentCmd);
});

listen('racc://session-stopped', async (event) => {
  const { session_id, source } = event.payload;
  if (source !== 'remote') return;
  killPty(session_id);
  await get().loadRepos();
});
```

### Sequence: Remote `create_session`

```
Remote Client                  Rust Backend                    Frontend
    │                              │                              │
    ├─ create_session ────────────▶│                              │
    │                              ├─ Create DB record            │
    │                              ├─ Create git worktree         │
    │                              ├─ broadcast(SessionCreated)   │
    │                              ├─ emit("racc://session-created", source:"remote")
    │◀─ { session_id } ───────────┤                              │
    │                              │                              │
    │                              │  ◀── listen() fires ────────┤
    │                              │                    spawnPty()│
    │                              │                              ├─ PTY running
    │                              │                              ├─ Agent starts
    │                              │                              │
    │◀─ event: session_status_changed (running) ─────────────────┤
```

### Sequence: Remote `stop_session`

```
Remote Client                  Rust Backend                    Frontend
    │                              │                              │
    ├─ stop_session ──────────────▶│                              │
    │                              ├─ Update DB → "Completed"    │
    │                              ├─ broadcast(SessionStopped)  │
    │                              ├─ emit("racc://session-stopped", source:"remote")
    │◀─ {} ───────────────────────┤                              │
    │                              │                              │
    │                              │  ◀── listen() fires ────────┤
    │                              │                    killPty() │
    │                              │                              ├─ PTY terminated
```

## File Changes

### New Files

- `src-tauri/src/ws_server.rs` — WebSocket server: listener, connection pool, message routing, event broadcasting, heartbeat
- `src-tauri/src/events.rs` — `RaccEvent` enum + broadcast channel type alias

### Modified Files

- `src-tauri/Cargo.toml` — Add `tokio-tungstenite`, `futures-util`
- `src-tauri/src/lib.rs` — Add `broadcast::Sender<RaccEvent>` to managed state; spawn WS server in `setup()`
- `src-tauri/src/commands/session.rs` — Emit `SessionStatusChanged` after DB writes in `create_session`, `stop_session`, `reattach_session`; add `source` field to Tauri events
- `src-tauri/src/commands/task.rs` — Emit `TaskStatusChanged` / `TaskDeleted` after DB writes
- `src/stores/sessionStore.ts` — Add `listen()` calls in `initialize()` for `racc://session-created` and `racc://session-stopped`

### Unchanged

- `src/services/ptyManager.ts` — PTY management stays in frontend (no API changes)

## Graceful Shutdown

On Tauri app exit:
1. The WS server tokio task receives cancellation via a `tokio::sync::watch` or `CancellationToken`
2. All connected clients receive a WebSocket close frame (code 1001, "server shutting down")
3. Connection pool is drained

## Future Considerations

- Token-based authentication (required before enabling `0.0.0.0` binding)
- Terminal output streaming (subscribe to PTY output per session)
- TLS support
- Configurable port via settings UI
- Method versioning / `get_capabilities` handshake
