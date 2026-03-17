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
│       │   ├── cost.rs
│       │   ├── db.rs         # init_db(), migrations, reset
│       │   ├── transport.rs  # write, resize, get_buffer, is_alive
│       │   └── insights.rs   # Session event recording, analysis
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
    pub terminal_tx: broadcast::Sender<TerminalData>,  // PTY output bus
}

/// Per-session terminal output, replaces app.emit("transport:data", ...)
pub struct TerminalData {
    pub session_id: i64,
    pub data: Vec<u8>,
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

**Terminal output** is the hardest decoupling point. Currently `LocalPtyTransport` and `SshTmuxTransport` call `app.emit("transport:data", ...)` from inside spawned background tasks. The replacement:

- `AppContext` owns a `terminal_tx: broadcast::Sender<TerminalData>` channel
- Transports receive a `terminal_tx.clone()` at spawn time (instead of `AppHandle`)
- Background reader tasks send `TerminalData { session_id, data }` to the broadcast
- Each backend subscribes and delivers appropriately:
  - **Tauri** subscribes and calls `app_handle.emit("transport:data", ...)` to feed IPC
  - **Axum** subscribes and sends binary WebSocket frames to connected clients
- `TransportManager::start_buffer_task()` also switches from `tauri::async_runtime::spawn` to `tokio::spawn`

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
    task_description: Option<String>,
    server_id: Option<String>,  // None = local PTY, Some = SSH/tmux remote
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

Stores (`sessionStore.ts`, `taskStore.ts`) call `transport.call(...)` instead of `invoke(...)`. There are ~28 `invoke()` call sites across ~11 files and ~8 Tauri plugin import sites that need conversion.

**Tauri plugin APIs in browser mode:** Some desktop-only APIs (`@tauri-apps/plugin-dialog`, `@tauri-apps/plugin-shell`, `@tauri-apps/plugin-notification`, `convertFileSrc`) are not available in the browser. For MVP, these are gracefully degraded:
- File picker dialog → hidden in browser mode
- Shell URL opening → `window.open()`
- Notifications → browser Notification API
- Asset protocol URLs → standard HTTP URLs served by axum

**Terminal data over WebSocket:** Binary frames tagged with an 8-byte session ID prefix (i64 LE). `WebSocketTransport` demuxes to the right `onTerminalData` handler per session.

---

## racc-server Binary

```rust
// racc-server/src/main.rs

#[tokio::main]
async fn main() {
    let db_path = std::env::var("RACC_DB_PATH")
        .map(PathBuf::from)
        .unwrap_or_else(|_| dirs::home_dir().unwrap().join(".racc/racc.db"));
    let event_bus = Arc::new(BroadcastEventBus::new());
    // AppContext::new() calls init_db() for schema + migrations
    let ctx = AppContext::new(db_path, event_bus).await;
    // Reconcile stale sessions from previous runs
    racc_core::commands::reconcile_sessions(&ctx).await;

    let app = Router::new()
        .route("/ws", get(ws_handler))
        .fallback_service(ServeDir::new("dist"))
        .with_state(Arc::new(ctx));

    let addr = SocketAddr::from(([0, 0, 0, 0], 9399));
    let listener = TcpListener::bind(addr).await.unwrap();
    println!("racc-server listening on http://0.0.0.0:9399");

    // Graceful shutdown on SIGTERM/SIGINT
    let shutdown = async {
        tokio::signal::ctrl_c().await.ok();
    };
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown)
        .await.unwrap();
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

JSON-RPC style protocol, evolved from the existing `ws_server.rs`.

**Request:**
```json
{"id": "1", "method": "create_session", "params": {"repo_id": 1, "use_worktree": true}}
```

**Response:**
```json
{"id": "1", "result": {"session_id": 42, "status": "running"}}
```

**Push event (no id):**
```json
{"event": "session_status_changed", "data": {"session_id": "abc-123", "status": "completed"}}
```

**Terminal data:** Binary WebSocket frames. First 8 bytes = session ID (i64 LE, matches DB type), remaining bytes = PTY output. This is a new addition — the existing `ws_server.rs` only handles text frames.

**MVP methods (carried over from existing API):**
- `create_session`, `stop_session`, `reattach_session`
- `create_task`, `list_tasks`, `update_task_status`, `update_task_description`, `delete_task`
- `list_repos`, `get_session_diff`

**New methods for terminal I/O:**
- `transport_write` — send input to a session's PTY
- `transport_resize` — resize a session's terminal
- `transport_get_buffer` — get buffered output for a session (on reconnect)

**Client reconnection flow:**
When a browser reconnects (tab refresh, device switch), the `WebSocketTransport` sends a `sync` message as its first request. The server responds with current state: active sessions, task list, and session statuses. The client then calls `transport_get_buffer` for each active session to restore terminal content.

---

## Migration Path for Existing Code

### Backend

1. Create `racc-core` crate, move `events.rs`, `db.rs`, `ssh/`, `transport/` as-is
2. Add `EventBus` trait, implement `BroadcastEventBus`
3. Add `terminal_tx` broadcast channel to `AppContext`, replace `AppHandle` in `LocalPtyTransport::spawn()` and `SshTmuxTransport::spawn()` with `terminal_tx.clone()`
4. Replace `tauri::async_runtime::spawn` with `tokio::spawn` in `TransportManager::start_buffer_task()`
5. Create `AppContext` struct, ensure `AppContext::new()` calls `init_db()`
6. Refactor all 10 command modules: strip `#[tauri::command]`, replace `State<T>` + `AppHandle` with `&AppContext`, replace `Result<T, String>` with `Result<T, CoreError>`
7. In existing Tauri app: add thin wrappers that call `racc-core` functions
8. Consolidate `ws_server.rs` — it currently reimplements business logic with raw SQL; replace with calls to `racc-core` functions
9. Create `racc-server` binary with axum, including startup (`init_db`, `reconcile_sessions`) and graceful shutdown

### Frontend

1. Create `RaccTransport` interface and `TauriTransport` / `WebSocketTransport` implementations
2. Replace all ~28 `invoke()` calls across stores, hooks, and components with `transport.call()`
3. Replace all `listen()` calls with `transport.on()`
4. Replace `transport:data` event listener with `transport.onTerminalData()`
5. Add WebSocket-based terminal write path (currently uses `invoke("transport_write", ...)`)
6. Add browser-mode fallbacks for Tauri plugin APIs (dialog, shell, notification, asset protocol)

### What Stays the Same

- React components — no changes to UI
- Task board logic — same API
- Git worktree management — same commands
- SSH/remote server management — same module, just moved to `racc-core`
- Database schema — unchanged
- WebSocket protocol — backward compatible with existing `racc-client.ts`
