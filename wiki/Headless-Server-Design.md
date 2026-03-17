# Headless Racc Server — Design Spec

## Goal

Extract Racc's backend into a standalone headless binary (`racc-server`) that serves the React UI over HTTP and exposes a WebSocket API, enabling multi-device access to persistent agent sessions over Tailscale.

## Use Case

Start agent sessions on a dev machine, connect from any device on the Tailscale network via browser. Sessions persist on the server — close the laptop, open on another device, pick up where you left off.

## Constraints

- Tailscale-only networking (no auth, no TLS — Tailscale handles both)
- MVP scope: task board + terminal streaming + session management
- Out of scope: file viewer, insights panel, assistant, daemonization
- No user-facing regressions in the existing Tauri desktop app

---

## Crate Structure

```
src-tauri/
├── racc-core/                # New lib crate — all business logic
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs            # AppContext, re-exports
│       ├── error.rs          # CoreError enum
│       ├── events.rs         # RaccEvent enum, EventBus trait, BroadcastEventBus
│       ├── db.rs             # SQLite setup + migrations
│       ├── commands/         # Plain async fns taking &AppContext
│       │   ├── mod.rs
│       │   ├── session.rs
│       │   ├── task.rs
│       │   ├── server.rs
│       │   ├── git.rs
│       │   └── cost.rs
│       ├── transport/        # Moved from src-tauri (Transport trait, manager)
│       │   ├── mod.rs        # Transport trait definition
│       │   ├── manager.rs    # TransportManager
│       │   ├── local_pty.rs  # LocalPtyTransport
│       │   └── ssh_tmux.rs   # SshTmuxTransport
│       └── ssh/              # Moved from src-tauri (zero changes)
│           ├── mod.rs
│           └── config_parser.rs
│
├── racc-server/              # New binary crate — headless server
│   ├── Cargo.toml
│   └── src/
│       ├── main.rs           # Tokio entrypoint, axum router
│       ├── ws.rs             # WebSocket handler + terminal streaming
│       └── http.rs           # Serve static React build
│
├── Cargo.toml                # Existing Tauri app (depends on racc-core)
└── src/                      # Thin #[tauri::command] wrappers
```

**Workspace:** The root `src-tauri/Cargo.toml` becomes a workspace with three members: `racc-core`, `racc-server`, and the existing Tauri app (renamed to `racc-tauri` internally).

---

## AppContext

Replaces scattered `State<T>` + `AppHandle` parameters across all commands.

```rust
// racc-core/src/lib.rs

pub struct AppContext {
    pub db: Arc<Mutex<Connection>>,
    pub transport_manager: TransportManager,
    pub ssh_manager: SshManager,
    pub event_bus: Arc<dyn EventBus>,
}
```

Both Tauri and axum construct their own `AppContext` at startup, providing their own `EventBus` implementation.

---

## EventBus Abstraction

The core decoupling point. Replaces `AppHandle.emit()`.

```rust
// racc-core/src/events.rs

#[async_trait]
pub trait EventBus: Send + Sync {
    async fn emit(&self, event: RaccEvent);
    fn subscribe(&self) -> broadcast::Receiver<RaccEvent>;
}

pub struct BroadcastEventBus {
    tx: broadcast::Sender<RaccEvent>,
}
```

- **`racc-server`** uses `BroadcastEventBus` directly — events go to WebSocket subscribers
- **`racc-tauri`** wraps it, adding `app_handle.emit("racc://event", &event)` so Tauri IPC still works alongside the broadcast

Terminal output follows the same pattern. Transports emit data through a channel owned by `AppContext`. Each backend decides delivery (Tauri IPC or WebSocket binary frames).

---

## Command Layer

Commands become plain async functions in `racc-core`:

```rust
// racc-core/src/commands/session.rs

pub async fn create_session(
    ctx: &AppContext,
    repo_id: i64,
    use_worktree: bool,
    branch: Option<String>,
    agent: Option<String>,
) -> Result<Session, CoreError> { ... }
```

Each backend wraps them thinly.

**Tauri wrapper:**
```rust
#[tauri::command]
pub async fn create_session(
    ctx: State<'_, AppContext>,
    repo_id: i64,
    use_worktree: bool,
    branch: Option<String>,
    agent: Option<String>,
) -> Result<Session, String> {
    racc_core::commands::create_session(&ctx, repo_id, use_worktree, branch, agent)
        .await
        .map_err(|e| e.to_string())
}
```

**Axum WebSocket handler:**
```rust
"create_session" => {
    let repo_id = params["repo_id"].as_i64()?;
    let use_worktree = params["use_worktree"].as_bool().unwrap_or(false);
    let branch = params.get("branch").and_then(|v| v.as_str()).map(String::from);
    let agent = params.get("agent").and_then(|v| v.as_str()).map(String::from);
    racc_core::commands::create_session(&ctx, repo_id, use_worktree, branch, agent).await?
}
```

**Error handling:** `CoreError` is a proper enum (not `String`). Each backend converts to its error type.

```rust
// racc-core/src/error.rs

#[derive(Debug, thiserror::Error)]
pub enum CoreError {
    #[error("Database error: {0}")]
    Db(#[from] rusqlite::Error),
    #[error("Transport error: {0}")]
    Transport(String),
    #[error("SSH error: {0}")]
    Ssh(String),
    #[error("Git error: {0}")]
    Git(String),
    #[error("Not found: {0}")]
    NotFound(String),
}
```

---

## Frontend Transport Layer

Abstracts Tauri IPC vs WebSocket behind a common interface.

```typescript
// src/services/transport.ts

interface RaccTransport {
  call(method: string, params: Record<string, unknown>): Promise<any>;
  on(event: string, handler: (data: any) => void): () => void;
  onTerminalData(sessionId: string, handler: (data: Uint8Array) => void): () => void;
}
```

**Two implementations:**
- `TauriTransport` — wraps `invoke()` and `listen()`, used in the desktop app
- `WebSocketTransport` — connects to `ws://<host>:9399/ws`, used in browser

**Auto-detection:**
```typescript
export const transport: RaccTransport =
  window.__TAURI_INTERNALS__
    ? new TauriTransport()
    : new WebSocketTransport(window.location.host);
```

Stores (`sessionStore.ts`, `taskStore.ts`) call `transport.call(...)` instead of `invoke(...)`.

**Terminal data over WebSocket:** Binary frames tagged with a 4-byte session ID prefix. `WebSocketTransport` demuxes to the right `onTerminalData` handler per session.

---

## racc-server Binary

```rust
// racc-server/src/main.rs

#[tokio::main]
async fn main() {
    let db_path = dirs::home_dir().unwrap().join(".racc/racc.db");
    let event_bus = Arc::new(BroadcastEventBus::new());
    let ctx = AppContext::new(db_path, event_bus).await;

    let app = Router::new()
        .route("/ws", get(ws_handler))
        .fallback_service(ServeDir::new("dist"))
        .with_state(Arc::new(ctx));

    let addr = SocketAddr::from(([0, 0, 0, 0], 9399));
    let listener = TcpListener::bind(addr).await.unwrap();
    println!("racc-server listening on http://0.0.0.0:9399");
    axum::serve(listener, app).await.unwrap();
}
```

- Serves React build (`dist/`) at `/` — browser opens, gets full UI
- WebSocket API at `/ws` — same protocol as existing `ws_server.rs`
- Binds `0.0.0.0:9399` — accessible on Tailscale network
- Single port for HTTP + WebSocket (axum handles upgrade)

**Build and run:**
```bash
bun run build                    # React → dist/
cargo build --bin racc-server    # Headless binary
./racc-server                    # Serve on :9399
```

---

## WebSocket Protocol

Same JSON-RPC style protocol as the existing `ws_server.rs`. No breaking changes.

**Request:**
```json
{"id": 1, "method": "create_session", "params": {"repo_id": 1, "use_worktree": true}}
```

**Response:**
```json
{"id": 1, "result": {"session_id": "abc-123", "status": "running"}}
```

**Push event (no id):**
```json
{"event": "session_status_changed", "data": {"session_id": "abc-123", "status": "completed"}}
```

**Terminal data:** Binary WebSocket frames. First 4 bytes = session ID (u32), remaining bytes = PTY output.

**MVP methods (carried over from existing API):**
- `create_session`, `stop_session`, `reattach_session`
- `create_task`, `list_tasks`, `update_task_status`, `update_task_description`, `delete_task`
- `list_repos`, `get_session_diff`

**New methods for terminal I/O:**
- `transport_write` — send input to a session's PTY
- `transport_resize` — resize a session's terminal
- `transport_get_buffer` — get buffered output for a session (on reconnect)

---

## Migration Path for Existing Code

### Backend

1. Create `racc-core` crate, move `events.rs`, `db.rs`, `ssh/`, `transport/` as-is
2. Add `EventBus` trait, implement `BroadcastEventBus`
3. Create `AppContext` struct
4. Refactor commands: strip `#[tauri::command]`, replace `State<T>` + `AppHandle` with `&AppContext`, replace `Result<T, String>` with `Result<T, CoreError>`
5. In existing Tauri app: add thin wrappers that call `racc-core` functions
6. Remove `AppHandle.emit()` calls from transport — use `EventBus` / output channels instead
7. Create `racc-server` binary with axum
8. Upgrade existing `ws_server.rs` dispatch to call `racc-core` functions

### Frontend

1. Create `RaccTransport` interface and two implementations
2. Replace all `invoke()` calls in stores/hooks with `transport.call()`
3. Replace all `listen()` calls with `transport.on()`
4. Replace `transport:data` event listener with `transport.onTerminalData()`
5. Add WebSocket-based terminal write path (currently uses `invoke("transport_write", ...)`)

### What Stays the Same

- React components — no changes to UI
- Task board logic — same API
- Git worktree management — same commands
- SSH/remote server management — same module, just moved to `racc-core`
- Database schema — unchanged
- WebSocket protocol — backward compatible with existing `racc-client.ts`
