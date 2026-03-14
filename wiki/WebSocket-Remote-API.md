# WebSocket Remote API

[< Home](Home.md) | [< Technical Architecture](Technical-Architecture.md)

## Overview

Racc exposes a WebSocket server on `ws://127.0.0.1:9399` that allows external agent clients (e.g., OpenClaw), CLI tools, or other devices to remotely create tasks, start sessions, and receive real-time status updates — without accessing the Racc UI.

The server starts automatically with Racc. No authentication is required (planned for a future release).

## Architecture

```
External Client ──WebSocket──▶ ws_server.rs ──▶ Existing command logic
                                                    │
                                              SQLite DB (shared)
                                                    │
                                          tokio::broadcast channel
                                                    │
                                    ┌───────────────┼───────────────┐
                                    ▼                               ▼
                              WebSocket broadcast            AppHandle.emit()
                              (to all clients)               (to frontend)
```

The WebSocket server is embedded in Tauri's Rust backend using `tokio-tungstenite`. It shares the same SQLite database as the UI, and all state mutations are broadcast to both WS clients and the frontend via a `tokio::broadcast` channel.

When a remote client creates a session, the frontend receives a Tauri event and automatically spawns the PTY + agent process — so the agent appears and runs in Racc's terminal UI.

## Message Protocol

All messages are JSON text frames.

### Request (Client → Racc)

```json
{
  "id": "req_1",
  "method": "create_task",
  "params": { "repo_id": 1, "description": "Fix login bug" }
}
```

### Response (Racc → Client)

```json
{
  "id": "req_1",
  "result": { "task_id": 5 }
}
```

### Error (Racc → Client)

```json
{
  "id": "req_1",
  "error": "Repo not found"
}
```

### Push Event (Racc → Client, no id)

```json
{
  "event": "session_status_changed",
  "data": { "session_id": 3, "status": "completed", "pr_url": "https://..." }
}
```

## Available Methods

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
| `create_session` | `repo_id`, `use_worktree`, `branch?`, `agent?` | `{ session_id }` |
| `stop_session` | `session_id` | `{}` |
| `reattach_session` | `session_id` | `{ session }` |

- `agent` defaults to `"claude-code"`. Remote sessions run with `--dangerously-skip-permissions`.
- `create_session` creates a git worktree (if `use_worktree: true`) and starts the agent in Racc's UI automatically.

### Query Operations

| Method | Params | Returns |
|--------|--------|---------|
| `list_repos` | — | `{ repos: [...] }` |
| `get_session_diff` | `session_id` | `{ diff: "..." }` |

### Push Events

| Event | Data |
|-------|------|
| `session_status_changed` | `{ session_id, status, pr_url? }` |
| `task_status_changed` | `{ task_id, status, session_id? }` |
| `task_deleted` | `{ task_id }` |

**Not exposed** (destructive): `remove_session`, `remove_repo`, `reset_db`.

## Quick Start

### Using websocat (CLI)

```bash
brew install websocat
websocat ws://127.0.0.1:9399
```

Then type JSON messages:

```json
{"id":"1","method":"list_repos","params":{}}
{"id":"2","method":"create_task","params":{"repo_id":1,"description":"Fix bug"}}
```

### Using the TypeScript SDK

See `examples/racc-client.ts` for a complete client SDK. Basic usage:

```typescript
import { RaccClient } from './racc-client';

const racc = new RaccClient();
await racc.connect();

// Subscribe to events
racc.on("session_status_changed", (data) => {
  console.log(`Session ${data.session_id} → ${data.status}`);
});

// Create a task and start a session
const { repos } = await racc.call("list_repos");
const { task_id } = await racc.call("create_task", {
  repo_id: repos[0].id,
  description: "Fix the login bug",
});
const { session_id } = await racc.call("create_session", {
  repo_id: repos[0].id,
  use_worktree: true,
  branch: "fix/login-bug",
});

// Stop when done
await racc.call("stop_session", { session_id });
racc.close();
```

### Using Python

```python
import asyncio, websockets, json

async def main():
    async with websockets.connect("ws://127.0.0.1:9399") as ws:
        await ws.send(json.dumps({
            "id": "1", "method": "list_repos", "params": {}
        }))
        resp = json.loads(await ws.recv())
        print(resp["result"]["repos"])

asyncio.run(main())
```

## Implementation Details

| Component | File | Purpose |
|-----------|------|---------|
| `events.rs` | `src-tauri/src/events.rs` | `RaccEvent` enum + broadcast channel |
| `ws_server.rs` | `src-tauri/src/ws_server.rs` | WebSocket server: listener, connection pool, heartbeat, handlers |
| `lib.rs` | `src-tauri/src/lib.rs` | Server spawn in `setup()`, graceful shutdown |
| `sessionStore.ts` | `src/stores/sessionStore.ts` | Frontend listeners for remote PTY bootstrap |
| Example client | `examples/racc-client.ts` | TypeScript WebSocket client SDK |

### Event Flow: Remote `create_session`

```
Remote Client                  Rust Backend                    Frontend
    │                              │                              │
    ├─ create_session ────────────▶│                              │
    │                              ├─ Create DB record            │
    │                              ├─ Create git worktree         │
    │                              ├─ broadcast(SessionCreated)   │
    │                              ├─ emit("racc://session-created")
    │◀─ { session_id } ───────────┤                              │
    │                              │  ◀── listen() fires ────────┤
    │                              │                    spawnPty()│
    │                              │                              ├─ Agent starts
    │◀─ event: session_status_changed ───────────────────────────┤
```

## Security Notes

- Binds to `127.0.0.1` (localhost only) by default — not accessible from other devices
- No authentication — planned for a future release (token-based)
- `0.0.0.0` binding will require authentication before being enabled
- Destructive operations are not exposed via the WebSocket API

## Future Considerations

- Token-based authentication
- Terminal output streaming (subscribe to PTY output per session)
- TLS support
- Configurable port via settings UI
- Method versioning / capability negotiation

[Next: Session Lifecycle >](Session-Lifecycle.md)
