# Headless Racc Server Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Extract Racc's Rust backend into a `racc-core` lib crate and build a standalone `racc-server` binary that serves the React UI + WebSocket API, enabling browser-based multi-device access over Tailscale.

**Architecture:** Three-crate Cargo workspace: `racc-core` (business logic), `racc-server` (axum binary), `racc-tauri` (existing desktop app, thin wrappers). Frontend gets a `RaccTransport` abstraction to switch between Tauri IPC and WebSocket.

**Tech Stack:** Rust, axum, tokio, rusqlite, russh, portable-pty, React 19, TypeScript, Zustand

**Spec:** `wiki/Headless-Server-Design.md`

**Critical notes from plan review:**
- Use `std::sync::Mutex` (not `tokio::sync::Mutex`) for DB in `AppContext` — `rusqlite::Connection` is `!Send`
- Use `default-members = ["."]` in workspace to avoid Tauri build issues
- Don't add `racc-server` to workspace members until Task 8 (it won't exist yet)
- `assistant.rs` stays in Tauri crate — NOT moved to racc-core (out of scope)
- Keep `.manage(SidecarState)` alongside `AppContext` in Tauri's lib.rs
- All static Tauri plugin imports MUST become dynamic `await import(...)` for browser compat
- Out-of-scope features (file viewer, insights, assistant) should degrade gracefully in browser (no crash)

---

## File Structure

### New Files

**racc-core crate:**
- `src-tauri/racc-core/Cargo.toml` — lib crate dependencies (no tauri)
- `src-tauri/racc-core/src/lib.rs` — `AppContext`, `TerminalData`, re-exports
- `src-tauri/racc-core/src/error.rs` — `CoreError` enum
- `src-tauri/racc-core/src/events.rs` — `RaccEvent`, `EventBus` trait, `BroadcastEventBus`
- `src-tauri/racc-core/src/db.rs` — `init_db()`, schema, migrations (from `commands/db.rs`)
- `src-tauri/racc-core/src/commands/mod.rs` — re-exports
- `src-tauri/racc-core/src/commands/session.rs` — session logic (from Tauri `commands/session.rs`)
- `src-tauri/racc-core/src/commands/task.rs` — task logic (from Tauri `commands/task.rs`)
- `src-tauri/racc-core/src/commands/server.rs` — server management (from Tauri `commands/server.rs`)
- `src-tauri/racc-core/src/commands/git.rs` — git operations (from Tauri `commands/git.rs`)
- `src-tauri/racc-core/src/commands/cost.rs` — cost tracking (from Tauri `commands/cost.rs`)
- `src-tauri/racc-core/src/commands/transport.rs` — transport commands (from Tauri `commands/transport.rs`)
- `src-tauri/racc-core/src/commands/insights.rs` — insights (from Tauri `commands/insights.rs`)
- `src-tauri/racc-core/src/commands/file.rs` — file operations (from Tauri `commands/file.rs`)
- `src-tauri/racc-core/src/transport/mod.rs` — `Transport` trait, `TransportError`, `RingBuffer`
- `src-tauri/racc-core/src/transport/manager.rs` — `TransportManager`
- `src-tauri/racc-core/src/transport/local_pty.rs` — `LocalPtyTransport`
- `src-tauri/racc-core/src/transport/ssh_tmux.rs` — `SshTmuxTransport`
- `src-tauri/racc-core/src/ssh/mod.rs` — `SshManager` (moved as-is)
- `src-tauri/racc-core/src/ssh/config_parser.rs` — SSH config parser (moved as-is)

**racc-server crate:**
- `src-tauri/racc-server/Cargo.toml` — binary crate, depends on `racc-core` + axum
- `src-tauri/racc-server/src/main.rs` — tokio entrypoint, axum router, graceful shutdown
- `src-tauri/racc-server/src/ws.rs` — WebSocket handler, terminal streaming, dispatch
- `src-tauri/racc-server/src/http.rs` — static file serving config

**Frontend:**
- `src/services/transport.ts` — `RaccTransport` interface, `TauriTransport`, `WebSocketTransport`, auto-detection

### Modified Files

**Tauri app (becomes thin wrappers):**
- `src-tauri/Cargo.toml` — becomes workspace root, add `racc-core` dependency
- `src-tauri/src/lib.rs` — create `AppContext`, wrap with Tauri event forwarding, thin command registration
- `src-tauri/src/commands/session.rs` — thin wrappers calling `racc_core::commands::*`
- `src-tauri/src/commands/task.rs` — thin wrappers
- `src-tauri/src/commands/server.rs` — thin wrappers
- `src-tauri/src/commands/transport.rs` — thin wrappers
- `src-tauri/src/commands/git.rs` — thin wrappers
- `src-tauri/src/commands/cost.rs` — thin wrappers
- `src-tauri/src/commands/insights.rs` — thin wrappers
- `src-tauri/src/commands/file.rs` — thin wrappers
- `src-tauri/src/commands/db.rs` — thin wrapper for `reset_db`
- `src-tauri/src/ws_server.rs` — dispatch calls `racc_core` functions instead of raw SQL

**Frontend stores/hooks (replace `invoke()` with `transport.call()`):**
- `src/stores/sessionStore.ts` — ~19 invoke sites + 2 listen sites
- `src/stores/taskStore.ts` — 5 invoke sites
- `src/stores/serverStore.ts` — 8 invoke sites
- `src/stores/insightsStore.ts` — 7 invoke sites + 1 listen site
- `src/stores/assistantStore.ts` — 7 invoke sites (out of MVP scope, but must not crash in browser — migrate invoke calls)
- `src/stores/fileViewerStore.ts` — 2 invoke sites (read_file, search_files)
- `src/services/ptyManager.ts` — 2 invoke sites
- `src/services/eventCapture.ts` — 1 invoke site
- `src/services/setupAgent.ts` — 2 invoke sites (execute_remote_command, list_servers)
- `src/hooks/usePtyBridge.ts` — 1 listen site for `transport:data`
- `src/services/ptyOutputParser.ts` — 1 listen site for `transport:data` (per-session tracking, needs cleanup refactor)
- `src/App.tsx` — 1 listen site for `menu-reset-db`
- `src/components/Terminal/Terminal.tsx` — 1 invoke site
- `src/components/Dashboard/StatusBar.tsx` — 1 invoke site (get_global_costs)
- `src/components/TaskBoard/TaskColumn.tsx` — 2 invoke sites
- `src/components/TaskBoard/TaskInput.tsx` — 3 invoke sites + 1 plugin import (dialog) + convertFileSrc
- `src/components/TaskBoard/TaskBoard.tsx` — 1 plugin import (dialog)
- `src/components/TaskBoard/TaskCard.tsx` — 1 plugin import (shell) + convertFileSrc
- `src/components/Sidebar/Sidebar.tsx` — 2 plugin imports (dialog, shell)
- `src/components/Sidebar/ImportRepoDialog.tsx` — 1 plugin import (dialog)
- `src/components/Insights/InsightActions.tsx` — 1 invoke site
- `src/components/SetupWizard/StaticGuide.tsx` — 1 invoke site (execute_remote_command)

---

## Chunk 1: racc-core Foundation

### Task 1: Create Cargo workspace and racc-core crate skeleton

**Files:**
- Modify: `src-tauri/Cargo.toml`
- Create: `src-tauri/racc-core/Cargo.toml`
- Create: `src-tauri/racc-core/src/lib.rs`
- Create: `src-tauri/racc-core/src/error.rs`

- [ ] **Step 1: Convert src-tauri/Cargo.toml to workspace root**

Add workspace section to `src-tauri/Cargo.toml` (the current crate is implicitly a member — do NOT list `"."`):

```toml
[workspace]
members = ["racc-core"]
default-members = ["."]
```

Note: `racc-server` is added to members later in Task 8 when the crate is created.

Add `racc-core` as a dependency:

```toml
[dependencies]
racc-core = { path = "racc-core" }
```

- [ ] **Step 2: Create racc-core/Cargo.toml**

```toml
[package]
name = "racc-core"
version = "0.1.0"
edition = "2021"

[dependencies]
serde = { version = "1", features = ["derive"] }
serde_json = "1"
tokio = { version = "1", features = ["full"] }
rusqlite = { version = "0.31", features = ["bundled"] }
chrono = { version = "0.4", features = ["serde"] }
portable-pty = "0.8"
async-trait = "0.1"
uuid = { version = "1", features = ["v4"] }
russh = "0.46"
russh-keys = "0.46"
dirs = "5"
log = "0.4"
thiserror = "2"
ignore = "0.4"
nucleo-matcher = "0.3"
strsim = "0.11"
```

- [ ] **Step 3: Create racc-core/src/error.rs**

```rust
use thiserror::Error;

#[derive(Debug, Error)]
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
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    Other(String),
}
```

- [ ] **Step 4: Create racc-core/src/lib.rs with AppContext and TerminalData**

```rust
pub mod commands;
pub mod db;
pub mod error;
pub mod events;
pub mod ssh;
pub mod transport;

use std::sync::{Arc, Mutex};
use rusqlite::Connection;
use tokio::sync::broadcast;

use crate::events::EventBus;
use crate::ssh::SshManager;
use crate::transport::manager::TransportManager;

pub use crate::error::CoreError;

#[derive(Clone, Debug, serde::Serialize)]
pub struct TerminalData {
    pub session_id: i64,
    pub data: Vec<u8>,
}

pub struct AppContext {
    // Note: uses std::sync::Mutex because rusqlite::Connection is !Send.
    // Lock must be dropped before any .await point.
    pub db: Arc<Mutex<Connection>>,
    pub transport_manager: TransportManager,
    pub ssh_manager: Arc<SshManager>,
    pub event_bus: Arc<dyn EventBus>,
    pub terminal_tx: broadcast::Sender<TerminalData>,
}

impl AppContext {
    pub fn new(
        db_path: std::path::PathBuf,
        event_bus: Arc<dyn EventBus>,
    ) -> Result<Self, CoreError> {
        let conn = crate::db::init_db(db_path)?;
        let (terminal_tx, _) = broadcast::channel(256);
        let transport_manager = TransportManager::new();
        let ssh_manager = Arc::new(SshManager::new());

        Ok(Self {
            db: Arc::new(Mutex::new(conn)),
            transport_manager,
            ssh_manager,
            event_bus,
            terminal_tx,
        })
    }
}
```

- [ ] **Step 5: Verify workspace compiles**

Run: `cd /home/devuser/racc/src-tauri && cargo check --workspace 2>&1 | head -20`

This will fail on missing modules — that's expected. We just need the workspace structure to be valid.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/Cargo.toml src-tauri/racc-core/
git commit -m "feat: create racc-core crate skeleton with AppContext and CoreError"
```

---

### Task 2: Move events system to racc-core

**Files:**
- Create: `src-tauri/racc-core/src/events.rs`
- Modify: `src-tauri/src/events.rs` — becomes a thin re-export + Tauri wrapper

- [ ] **Step 1: Create racc-core/src/events.rs with EventBus trait**

Copy the `RaccEvent` enum from `src-tauri/src/events.rs` (lines 9-28), then add the `EventBus` trait and `BroadcastEventBus`:

```rust
use async_trait::async_trait;
use serde::Serialize;
use tokio::sync::broadcast;

#[derive(Clone, Debug, Serialize)]
#[serde(tag = "event", content = "data")]
pub enum RaccEvent {
    SessionStatusChanged {
        session_id: i64,
        status: String,
        pr_url: Option<String>,
        #[serde(skip)]
        source: String,
    },
    TaskStatusChanged {
        task_id: i64,
        status: String,
        session_id: Option<i64>,
    },
    TaskDeleted {
        task_id: i64,
    },
}

#[async_trait]
pub trait EventBus: Send + Sync {
    async fn emit(&self, event: RaccEvent);
    fn subscribe(&self) -> broadcast::Receiver<RaccEvent>;
}

pub struct BroadcastEventBus {
    tx: broadcast::Sender<RaccEvent>,
}

impl BroadcastEventBus {
    pub fn new() -> Self {
        let (tx, _) = broadcast::channel(64);
        Self { tx }
    }
}

#[async_trait]
impl EventBus for BroadcastEventBus {
    async fn emit(&self, event: RaccEvent) {
        let _ = self.tx.send(event);
    }

    fn subscribe(&self) -> broadcast::Receiver<RaccEvent> {
        self.tx.subscribe()
    }
}
```

- [ ] **Step 2: Update src-tauri/src/events.rs to re-export from racc-core**

Replace the contents of `src-tauri/src/events.rs` with:

```rust
pub use racc_core::events::{RaccEvent, EventBus, BroadcastEventBus};

// Kept for backward compat with existing Tauri code during migration
pub type EventSender = tokio::sync::broadcast::Sender<RaccEvent>;

pub fn create_event_bus() -> (EventSender, tokio::sync::broadcast::Receiver<RaccEvent>) {
    tokio::sync::broadcast::channel(64)
}
```

- [ ] **Step 3: Verify it compiles**

Run: `cd /home/devuser/racc/src-tauri && cargo check 2>&1 | head -20`

- [ ] **Step 4: Commit**

```bash
git add src-tauri/racc-core/src/events.rs src-tauri/src/events.rs
git commit -m "feat: move events system to racc-core with EventBus trait"
```

---

### Task 3: Move SSH module to racc-core

**Files:**
- Create: `src-tauri/racc-core/src/ssh/mod.rs`
- Create: `src-tauri/racc-core/src/ssh/config_parser.rs`
- Modify: `src-tauri/src/ssh/mod.rs` — becomes re-export

The SSH module has zero Tauri coupling, so this is a straight move.

- [ ] **Step 1: Copy SSH files to racc-core**

Copy `src-tauri/src/ssh/mod.rs` → `src-tauri/racc-core/src/ssh/mod.rs`
Copy `src-tauri/src/ssh/config_parser.rs` → `src-tauri/racc-core/src/ssh/config_parser.rs`

No changes needed to the file contents.

- [ ] **Step 2: Update src-tauri/src/ssh/mod.rs to re-export**

Replace with:

```rust
pub use racc_core::ssh::*;
```

Also update `config_parser.rs`:

```rust
pub use racc_core::ssh::config_parser::*;
```

- [ ] **Step 3: Verify it compiles**

Run: `cd /home/devuser/racc/src-tauri && cargo check 2>&1 | head -20`

- [ ] **Step 4: Commit**

```bash
git add src-tauri/racc-core/src/ssh/ src-tauri/src/ssh/
git commit -m "feat: move SSH module to racc-core (zero changes)"
```

---

### Task 4: Move database module to racc-core

**Files:**
- Create: `src-tauri/racc-core/src/db.rs`
- Modify: `src-tauri/src/commands/db.rs` — becomes thin wrapper

- [ ] **Step 1: Create racc-core/src/db.rs**

Extract `init_db()` from `src-tauri/src/commands/db.rs` (lines 16-102). Change it to accept a `PathBuf` and return `Result<Connection, CoreError>`:

```rust
use std::path::PathBuf;
use rusqlite::Connection;
use crate::error::CoreError;

pub fn init_db(db_path: PathBuf) -> Result<Connection, CoreError> {
    if let Some(parent) = db_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let conn = Connection::open(&db_path)?;
    conn.execute_batch("PRAGMA journal_mode=WAL;")?;

    // Copy the full schema creation from src-tauri/src/commands/db.rs lines 25-102
    // (repos, sessions, tasks, session_events, insights, servers tables)
    // Replace all .map_err(|e| e.to_string())? with just ?

    Ok(conn)
}

pub fn reset_db(conn: &Connection) -> Result<(), CoreError> {
    // Copy reset logic from src-tauri/src/commands/db.rs
    conn.execute_batch(
        "DELETE FROM session_events;
         DELETE FROM insights;
         DELETE FROM tasks;
         DELETE FROM sessions;
         DELETE FROM repos;
         DELETE FROM servers;"
    )?;
    Ok(())
}
```

- [ ] **Step 2: Update src-tauri/src/commands/db.rs to wrap racc-core**

Replace `init_db()` body with a call to `racc_core::db::init_db()` using the existing path logic. Keep `#[tauri::command] reset_db` as a thin wrapper.

- [ ] **Step 3: Verify it compiles**

Run: `cd /home/devuser/racc/src-tauri && cargo check 2>&1 | head -20`

- [ ] **Step 4: Commit**

```bash
git add src-tauri/racc-core/src/db.rs src-tauri/src/commands/db.rs
git commit -m "feat: move database init and schema to racc-core"
```

---

### Task 5: Move transport system to racc-core

**Files:**
- Create: `src-tauri/racc-core/src/transport/mod.rs`
- Create: `src-tauri/racc-core/src/transport/manager.rs`
- Create: `src-tauri/racc-core/src/transport/local_pty.rs`
- Create: `src-tauri/racc-core/src/transport/ssh_tmux.rs`

This is the hardest task — transports currently take `AppHandle` for `app.emit("transport:data", ...)`. We replace that with `broadcast::Sender<TerminalData>`.

- [ ] **Step 1: Copy transport/mod.rs to racc-core**

Copy `src-tauri/src/transport/mod.rs` → `src-tauri/racc-core/src/transport/mod.rs`

Remove any Tauri imports. The `Transport` trait, `TransportError`, and `RingBuffer` have no Tauri coupling.

- [ ] **Step 2: Copy and refactor transport/manager.rs**

Copy to `racc-core/src/transport/manager.rs`. Replace `tauri::async_runtime::spawn` (line 30) with `tokio::spawn`:

```rust
// Before:
tauri::async_runtime::spawn(async move { ... });

// After:
tokio::spawn(async move { ... });
```

No other changes needed — `TransportManager` uses tokio primitives.

- [ ] **Step 3: Copy and refactor transport/local_pty.rs**

Copy to `racc-core/src/transport/local_pty.rs`. Replace `AppHandle` parameter with `broadcast::Sender<TerminalData>`:

```rust
// Before (line 16-24):
pub async fn spawn(
    session_id: i64, cwd: &str, cmd: &str,
    cols: u16, rows: u16,
    app: AppHandle,
    buffer_tx: UnboundedSender<(i64, Vec<u8>)>,
) -> Result<Self, TransportError>

// After:
pub async fn spawn(
    session_id: i64, cwd: &str, cmd: &str,
    cols: u16, rows: u16,
    terminal_tx: broadcast::Sender<crate::TerminalData>,
    buffer_tx: UnboundedSender<(i64, Vec<u8>)>,
) -> Result<Self, TransportError>
```

In the background reader task (line 56), replace:
```rust
// Before:
let _ = app.emit("transport:data", serde_json::json!({...}));

// After:
let _ = terminal_tx.send(crate::TerminalData { session_id: sid, data: data.clone() });
```

Remove all `use tauri::*` imports.

- [ ] **Step 4: Copy and refactor transport/ssh_tmux.rs**

Same pattern as local_pty.rs. Replace `AppHandle` with `broadcast::Sender<TerminalData>` in `spawn()`. Replace `app.emit("transport:data", ...)` calls (lines 102, 114-119) with `terminal_tx.send(TerminalData {...})`.

Remove all `use tauri::*` imports.

- [ ] **Step 5: Update src-tauri/src/transport/ to re-export from racc-core**

Replace each file in `src-tauri/src/transport/` with re-exports:

`src-tauri/src/transport/mod.rs`:
```rust
pub use racc_core::transport::*;
```

Same for `manager.rs`, `local_pty.rs`, `ssh_tmux.rs`.

- [ ] **Step 6: Update call sites in session.rs**

In `src-tauri/src/commands/session.rs`, update `LocalPtyTransport::spawn()` and `SshTmuxTransport::spawn()` calls to pass `terminal_tx` instead of `app_handle`. Get `terminal_tx` from `AppContext` or pass it as a managed state.

- [ ] **Step 7: Verify it compiles**

Run: `cd /home/devuser/racc/src-tauri && cargo check 2>&1 | head -20`

- [ ] **Step 8: Commit**

```bash
git add src-tauri/racc-core/src/transport/ src-tauri/src/transport/ src-tauri/src/commands/session.rs
git commit -m "feat: move transport system to racc-core, replace AppHandle with terminal_tx"
```

---

### Task 6: Move command modules to racc-core

**Files:**
- Create: `src-tauri/racc-core/src/commands/mod.rs`
- Create: `src-tauri/racc-core/src/commands/session.rs`
- Create: `src-tauri/racc-core/src/commands/task.rs`
- Create: `src-tauri/racc-core/src/commands/server.rs`
- Create: `src-tauri/racc-core/src/commands/git.rs`
- Create: `src-tauri/racc-core/src/commands/cost.rs`
- Create: `src-tauri/racc-core/src/commands/transport.rs`
- Create: `src-tauri/racc-core/src/commands/insights.rs`
- Create: `src-tauri/racc-core/src/commands/file.rs`

For each command module, the pattern is:
1. Copy to racc-core
2. Remove `#[tauri::command]`
3. Replace `State<'_, T>` params with `&AppContext`
4. Replace `app_handle.try_state::<EventSender>()` event emission with `ctx.event_bus.emit()`
5. Replace `Result<T, String>` with `Result<T, CoreError>`

- [ ] **Step 1: Create racc-core/src/commands/mod.rs**

```rust
pub mod session;
pub mod task;
pub mod server;
pub mod git;
pub mod cost;
pub mod transport;
pub mod insights;
pub mod file;
```

- [ ] **Step 2: Extract session commands**

Copy `src-tauri/src/commands/session.rs` to `racc-core/src/commands/session.rs`.

For each function (import_repo, list_repos, create_session, stop_session, reattach_session, reconcile_sessions, etc.):
- Remove `#[tauri::command]`
- Replace parameter list with `ctx: &AppContext` + business params
- Replace `db.lock()` with `ctx.db.lock().await`
- Replace event emission:
  ```rust
  // Before:
  if let Some(tx) = app_handle.try_state::<EventSender>() {
      let _ = tx.send(RaccEvent::SessionStatusChanged { ... });
  }
  // After:
  ctx.event_bus.emit(RaccEvent::SessionStatusChanged { ... }).await;
  ```
- Replace transport spawn calls to pass `ctx.terminal_tx.clone()` instead of `app_handle`
- Replace `Result<T, String>` with `Result<T, CoreError>`

Key functions to extract:
- `import_repo(ctx, path) -> Result<Repo, CoreError>`
- `list_repos(ctx) -> Result<Vec<RepoWithSessions>, CoreError>`
- `create_session(ctx, repo_id, use_worktree, branch, agent, task_description, server_id) -> Result<Session, CoreError>`
- `stop_session(ctx, session_id) -> Result<(), CoreError>`
- `reattach_session(ctx, session_id) -> Result<Session, CoreError>`
- `reconcile_sessions(ctx) -> Result<Vec<RepoWithSessions>, CoreError>`
- `get_session_diff(ctx, session_id) -> Result<String, CoreError>` — NEW: resolve worktree path from session, call `git diff HEAD`. Port from `ws_server.rs` handler `handle_get_session_diff`.
- `update_session_pr_url(ctx, session_id, pr_url) -> Result<(), CoreError>`
- `remove_repo(ctx, repo_id) -> Result<(), CoreError>`
- `remove_session(ctx, session_id, delete_worktree) -> Result<(), CoreError>`

Also move the struct definitions (SessionStatus, Repo, Session, RepoWithSessions) to this file.

- [ ] **Step 3: Extract task commands**

Copy and refactor `task.rs`. Key functions:
- `create_task(ctx, repo_id, description, images) -> Result<Task, CoreError>`
- `list_tasks(ctx, repo_id) -> Result<Vec<Task>, CoreError>`
- `update_task_status(ctx, task_id, status, session_id) -> Result<(), CoreError>`
- `update_task_description(ctx, task_id, description) -> Result<(), CoreError>`
- `delete_task(ctx, task_id) -> Result<(), CoreError>`
- Plus image-related helpers: `save_task_image`, `delete_task_image`, `rename_task_image`, `update_task_images`, `copy_file_to_task_images`

Replace event emission pattern (same as session.rs).

- [ ] **Step 4: Extract remaining command modules**

For each of `server.rs`, `git.rs`, `cost.rs`, `transport.rs`, `insights.rs`, `file.rs`:
- Same refactoring pattern
- `git.rs` and `cost.rs` are simplest — they have no `State<>` dependencies, just remove `#[tauri::command]` and change error types
- `server.rs` — replace `State<'_, Arc<Mutex<Connection>>>` and `State<'_, Arc<SshManager>>` with `&AppContext`
- `transport.rs` — replace `State<'_, TransportManager>` with `&AppContext`
- `insights.rs` — replace `State<>` + `AppHandle` with `&AppContext`, replace `app.emit()` with `ctx.event_bus.emit()`
- `file.rs` — replace `State<>` with `&AppContext`

- [ ] **Step 5: Verify racc-core compiles**

Run: `cd /home/devuser/racc/src-tauri && cargo check -p racc-core 2>&1 | head -30`

Fix any compilation errors iteratively.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/racc-core/src/commands/
git commit -m "feat: extract all command modules to racc-core"
```

---

### Task 7: Create Tauri thin wrappers

**Files:**
- Modify: `src-tauri/src/commands/session.rs`
- Modify: `src-tauri/src/commands/task.rs`
- Modify: `src-tauri/src/commands/server.rs`
- Modify: `src-tauri/src/commands/git.rs`
- Modify: `src-tauri/src/commands/cost.rs`
- Modify: `src-tauri/src/commands/transport.rs`
- Modify: `src-tauri/src/commands/insights.rs`
- Modify: `src-tauri/src/commands/file.rs`
- Modify: `src-tauri/src/commands/db.rs`
- Modify: `src-tauri/src/lib.rs`

- [ ] **Step 1: Replace each Tauri command module with thin wrappers**

For each command file in `src-tauri/src/commands/`, replace the full implementation with thin wrappers. Example for session.rs:

```rust
use tauri::State;
use racc_core::{AppContext, CoreError};
use racc_core::commands::session::{Repo, Session, RepoWithSessions};

#[tauri::command]
pub async fn import_repo(
    ctx: State<'_, AppContext>,
    path: String,
) -> Result<Repo, String> {
    racc_core::commands::session::import_repo(&ctx, path)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn create_session(
    ctx: State<'_, AppContext>,
    repo_id: i64,
    use_worktree: bool,
    branch: Option<String>,
    agent: Option<String>,
    task_description: Option<String>,
    server_id: Option<String>,
) -> Result<Session, String> {
    racc_core::commands::session::create_session(
        &ctx, repo_id, use_worktree, branch, agent, task_description, server_id,
    ).await.map_err(|e| e.to_string())
}

// ... same pattern for all other commands
```

- [ ] **Step 2: Update lib.rs to manage AppContext**

Replace the scattered `.manage()` calls with `AppContext` + keep `SidecarState` separately (assistant stays in Tauri):

```rust
// Create AppContext (replaces db_arc, event_tx, transport_manager, ssh_manager)
let event_bus = Arc::new(BroadcastEventBus::new());
let app_context = AppContext::new(db_path, event_bus).expect("Failed to init AppContext");
app_context.transport_manager.start_buffer_task();

// ...in tauri::Builder setup:
.manage(app_context)
.manage(tokio::sync::Mutex::new(commands::assistant::SidecarState::new()))  // Keep for assistant
```

Then in the `setup` closure, spawn two forwarder tasks:

```rust
// Terminal data forwarder: racc-core broadcast → Tauri IPC
let app_handle = app.handle().clone();
let mut terminal_rx = app.state::<AppContext>().terminal_tx.subscribe();
tauri::async_runtime::spawn(async move {
    while let Ok(data) = terminal_rx.recv().await {
        let _ = app_handle.emit("transport:data", serde_json::json!({
            "session_id": data.session_id,
            "data": data.data,
        }));
    }
});

// Event bus forwarder: racc-core broadcast → Tauri IPC
let app_handle2 = app.handle().clone();
let mut event_rx = app.state::<AppContext>().event_bus.subscribe();
tauri::async_runtime::spawn(async move {
    while let Ok(event) = event_rx.recv().await {
        let _ = app_handle2.emit("racc://event", &event);
    }
});
```

This preserves the existing frontend behavior — the Tauri app receives events via IPC as before.

- [ ] **Step 3: Verify the Tauri app compiles**

Run: `cd /home/devuser/racc/src-tauri && cargo check 2>&1 | head -30`

- [ ] **Step 4: Verify the Tauri app runs**

Run: `cd /home/devuser/racc && bun tauri dev`

Test that sessions can be created and terminal output appears.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/
git commit -m "feat: replace Tauri commands with thin wrappers over racc-core"
```

---

## Chunk 2: racc-server Binary

### Task 8: Create racc-server crate with axum

**Files:**
- Create: `src-tauri/racc-server/Cargo.toml`
- Create: `src-tauri/racc-server/src/main.rs`
- Create: `src-tauri/racc-server/src/http.rs`

- [ ] **Step 1: Add racc-server to workspace members**

In `src-tauri/Cargo.toml`, update the workspace section:
```toml
[workspace]
members = ["racc-core", "racc-server"]
default-members = ["."]
```

- [ ] **Step 2: Create racc-server/Cargo.toml**

```toml
[package]
name = "racc-server"
version = "0.1.0"
edition = "2021"

[[bin]]
name = "racc-server"
path = "src/main.rs"

[dependencies]
racc-core = { path = "../racc-core" }
axum = { version = "0.8", features = ["ws"] }
tokio = { version = "1", features = ["full"] }
tower-http = { version = "0.6", features = ["fs", "cors"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
dirs = "5"
futures-util = { version = "0.3", default-features = false, features = ["sink"] }
log = "0.4"
env_logger = "0.11"
```

- [ ] **Step 2: Create racc-server/src/http.rs**

```rust
use tower_http::services::ServeDir;

pub fn static_file_service(dist_path: &str) -> ServeDir {
    ServeDir::new(dist_path)
}
```

- [ ] **Step 3: Create racc-server/src/main.rs**

```rust
mod http;
mod ws;

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use axum::{routing::get, Router};
use racc_core::{AppContext, events::BroadcastEventBus};
use tokio::net::TcpListener;

#[tokio::main]
async fn main() {
    env_logger::init();

    let db_path = std::env::var("RACC_DB_PATH")
        .map(PathBuf::from)
        .unwrap_or_else(|_| dirs::home_dir().unwrap().join(".racc/racc.db"));

    let dist_path = std::env::var("RACC_DIST_PATH")
        .unwrap_or_else(|_| "dist".to_string());

    let event_bus = Arc::new(BroadcastEventBus::new());
    let ctx = AppContext::new(db_path, event_bus)
        .await
        .expect("Failed to initialize AppContext");

    // Reconcile stale sessions
    if let Err(e) = racc_core::commands::session::reconcile_sessions(&ctx).await {
        eprintln!("Warning: session reconciliation failed: {}", e);
    }

    // Start transport buffer task
    ctx.transport_manager.start_buffer_task();

    let ctx = Arc::new(ctx);

    let app = Router::new()
        .route("/ws", get(ws::ws_handler))
        .fallback_service(http::static_file_service(&dist_path))
        .with_state(ctx);

    let port: u16 = std::env::var("RACC_PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(9399);
    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    let listener = TcpListener::bind(addr).await.unwrap();
    println!("racc-server listening on http://{}", addr);

    let shutdown = async {
        tokio::signal::ctrl_c().await.ok();
        println!("\nShutting down...");
    };
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown)
        .await
        .unwrap();
}
```

- [ ] **Step 4: Verify it compiles**

Run: `cd /home/devuser/racc/src-tauri && cargo check -p racc-server 2>&1 | head -20`

- [ ] **Step 5: Commit**

```bash
git add src-tauri/racc-server/
git commit -m "feat: create racc-server binary crate with axum skeleton"
```

---

### Task 9: Implement WebSocket handler for racc-server

**Files:**
- Create: `src-tauri/racc-server/src/ws.rs`

Port the dispatch logic from `src-tauri/src/ws_server.rs` to call `racc_core` functions instead of raw SQL.

- [ ] **Step 1: Create racc-server/src/ws.rs with WebSocket upgrade and dispatch**

```rust
use std::sync::Arc;
use axum::{
    extract::{State, ws::{Message, WebSocket, WebSocketUpgrade}},
    response::IntoResponse,
};
use futures_util::{SinkExt, StreamExt};
use racc_core::{AppContext, TerminalData};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::broadcast;

#[derive(Deserialize)]
struct Request {
    id: String,
    method: String,
    #[serde(default)]
    params: Value,
}

#[derive(Serialize)]
struct Response {
    id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

pub async fn ws_handler(
    ws: WebSocketUpgrade,
    State(ctx): State<Arc<AppContext>>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, ctx))
}

async fn handle_socket(socket: WebSocket, ctx: Arc<AppContext>) {
    let (mut sender, mut receiver) = socket.split();
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<Message>();

    // Event forwarder: broadcast events → this client
    let mut event_rx = ctx.event_bus.subscribe();
    let event_tx = tx.clone();
    tokio::spawn(async move {
        while let Ok(event) = event_rx.recv().await {
            let msg = serde_json::to_string(&event).unwrap_or_default();
            if event_tx.send(Message::Text(msg.into())).is_err() {
                break;
            }
        }
    });

    // Terminal data forwarder: binary frames → this client
    let mut terminal_rx = ctx.terminal_tx.subscribe();
    let terminal_tx = tx.clone();
    tokio::spawn(async move {
        while let Ok(data) = terminal_rx.recv().await {
            let mut frame = Vec::with_capacity(8 + data.data.len());
            frame.extend_from_slice(&data.session_id.to_le_bytes());
            frame.extend_from_slice(&data.data);
            if terminal_tx.send(Message::Binary(frame.into())).is_err() {
                break;
            }
        }
    });

    // Sender task: mpsc → websocket
    let send_task = tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            if sender.send(msg).await.is_err() {
                break;
            }
        }
    });

    // Receiver loop: websocket → dispatch
    while let Some(Ok(msg)) = receiver.next().await {
        if let Message::Text(text) = msg {
            let response = match serde_json::from_str::<Request>(&text) {
                Ok(req) => {
                    match dispatch(&ctx, &req.method, &req.params).await {
                        Ok(result) => Response { id: req.id, result: Some(result), error: None },
                        Err(e) => Response { id: req.id, result: None, error: Some(e) },
                    }
                }
                Err(e) => Response {
                    id: "unknown".to_string(),
                    result: None,
                    error: Some(format!("Invalid request: {}", e)),
                },
            };
            let msg = serde_json::to_string(&response).unwrap_or_default();
            if tx.send(Message::Text(msg.into())).is_err() {
                break;
            }
        }
    }

    send_task.abort();
}

async fn dispatch(ctx: &AppContext, method: &str, params: &Value) -> Result<Value, String> {
    match method {
        // Sync method for client reconnection
        "sync" => {
            let repos = racc_core::commands::session::list_repos(ctx).await.map_err(|e| e.to_string())?;
            Ok(serde_json::to_value(repos).unwrap())
        }

        // Session methods
        "create_session" => {
            let repo_id = params["repo_id"].as_i64().ok_or("missing repo_id")?;
            let use_worktree = params["use_worktree"].as_bool().unwrap_or(false);
            let branch = params.get("branch").and_then(|v| v.as_str()).map(String::from);
            let agent = params.get("agent").and_then(|v| v.as_str()).map(String::from);
            let task_description = params.get("task_description").and_then(|v| v.as_str()).map(String::from);
            let server_id = params.get("server_id").and_then(|v| v.as_str()).map(String::from);
            let session = racc_core::commands::session::create_session(
                ctx, repo_id, use_worktree, branch, agent, task_description, server_id,
            ).await.map_err(|e| e.to_string())?;
            Ok(serde_json::to_value(session).unwrap())
        }
        "stop_session" => {
            let session_id = params["session_id"].as_i64().ok_or("missing session_id")?;
            racc_core::commands::session::stop_session(ctx, session_id).await.map_err(|e| e.to_string())?;
            Ok(Value::Null)
        }
        "reattach_session" => {
            let session_id = params["session_id"].as_i64().ok_or("missing session_id")?;
            let session = racc_core::commands::session::reattach_session(ctx, session_id).await.map_err(|e| e.to_string())?;
            Ok(serde_json::to_value(session).unwrap())
        }
        "list_repos" => {
            let repos = racc_core::commands::session::list_repos(ctx).await.map_err(|e| e.to_string())?;
            Ok(serde_json::to_value(repos).unwrap())
        }
        "get_session_diff" => {
            let session_id = params["session_id"].as_i64().ok_or("missing session_id")?;
            // Look up worktree path from session, then call git diff
            let diff = racc_core::commands::session::get_session_diff(ctx, session_id).await.map_err(|e| e.to_string())?;
            Ok(serde_json::to_value(diff).unwrap())
        }

        // Task methods
        "create_task" => {
            let repo_id = params["repo_id"].as_i64().ok_or("missing repo_id")?;
            let description = params["description"].as_str().ok_or("missing description")?.to_string();
            let images = params.get("images").and_then(|v| v.as_str()).map(String::from);
            let task = racc_core::commands::task::create_task(ctx, repo_id, description, images).await.map_err(|e| e.to_string())?;
            Ok(serde_json::to_value(task).unwrap())
        }
        "list_tasks" => {
            let repo_id = params["repo_id"].as_i64().ok_or("missing repo_id")?;
            let tasks = racc_core::commands::task::list_tasks(ctx, repo_id).await.map_err(|e| e.to_string())?;
            Ok(serde_json::to_value(tasks).unwrap())
        }
        "update_task_status" => {
            let task_id = params["task_id"].as_i64().ok_or("missing task_id")?;
            let status = params["status"].as_str().ok_or("missing status")?.to_string();
            let session_id = params.get("session_id").and_then(|v| v.as_i64());
            racc_core::commands::task::update_task_status(ctx, task_id, status, session_id).await.map_err(|e| e.to_string())?;
            Ok(Value::Null)
        }
        "update_task_description" => {
            let task_id = params["task_id"].as_i64().ok_or("missing task_id")?;
            let description = params["description"].as_str().ok_or("missing description")?.to_string();
            racc_core::commands::task::update_task_description(ctx, task_id, description).await.map_err(|e| e.to_string())?;
            Ok(Value::Null)
        }
        "delete_task" => {
            let task_id = params["task_id"].as_i64().ok_or("missing task_id")?;
            racc_core::commands::task::delete_task(ctx, task_id).await.map_err(|e| e.to_string())?;
            Ok(Value::Null)
        }

        // Transport methods
        "transport_write" => {
            let session_id = params["session_id"].as_i64().ok_or("missing session_id")?;
            let data = params["data"].as_array().ok_or("missing data")?
                .iter().filter_map(|v| v.as_u64().map(|n| n as u8)).collect::<Vec<u8>>();
            racc_core::commands::transport::transport_write(ctx, session_id, data).await.map_err(|e| e.to_string())?;
            Ok(Value::Null)
        }
        "transport_resize" => {
            let session_id = params["session_id"].as_i64().ok_or("missing session_id")?;
            let cols = params["cols"].as_u64().ok_or("missing cols")? as u16;
            let rows = params["rows"].as_u64().ok_or("missing rows")? as u16;
            racc_core::commands::transport::transport_resize(ctx, session_id, cols, rows).await.map_err(|e| e.to_string())?;
            Ok(Value::Null)
        }
        "transport_get_buffer" => {
            let session_id = params["session_id"].as_i64().ok_or("missing session_id")?;
            let buffer = racc_core::commands::transport::transport_get_buffer(ctx, session_id).await.map_err(|e| e.to_string())?;
            Ok(serde_json::to_value(buffer).unwrap())
        }

        // Additional session methods
        "import_repo" => {
            let path = params["path"].as_str().ok_or("missing path")?.to_string();
            let repo = racc_core::commands::session::import_repo(&ctx, path).await.map_err(|e| e.to_string())?;
            Ok(serde_json::to_value(repo).unwrap())
        }
        "remove_repo" => {
            let repo_id = params["repo_id"].as_i64().ok_or("missing repo_id")?;
            racc_core::commands::session::remove_repo(ctx, repo_id).await.map_err(|e| e.to_string())?;
            Ok(Value::Null)
        }
        "remove_session" => {
            let session_id = params["session_id"].as_i64().ok_or("missing session_id")?;
            let delete_worktree = params.get("delete_worktree").and_then(|v| v.as_bool()).unwrap_or(true);
            racc_core::commands::session::remove_session(ctx, session_id, delete_worktree).await.map_err(|e| e.to_string())?;
            Ok(Value::Null)
        }
        "update_session_pr_url" => {
            let session_id = params["session_id"].as_i64().ok_or("missing session_id")?;
            let pr_url = params["pr_url"].as_str().ok_or("missing pr_url")?.to_string();
            racc_core::commands::session::update_session_pr_url(ctx, session_id, pr_url).await.map_err(|e| e.to_string())?;
            Ok(Value::Null)
        }
        "reconcile_sessions" => {
            let repos = racc_core::commands::session::reconcile_sessions(ctx).await.map_err(|e| e.to_string())?;
            Ok(serde_json::to_value(repos).unwrap())
        }
        "reset_db" => {
            racc_core::db::reset_db_from_ctx(ctx).map_err(|e| e.to_string())?;
            Ok(Value::Null)
        }

        // Server management
        "list_servers" => {
            let servers = racc_core::commands::server::list_servers(ctx).await.map_err(|e| e.to_string())?;
            Ok(serde_json::to_value(servers).unwrap())
        }
        "add_server" | "update_server" | "remove_server" | "connect_server"
        | "disconnect_server" | "test_connection" | "list_ssh_config_hosts" => {
            // Route to respective racc_core::commands::server functions
            // Pattern: extract params, call function, return result
            racc_core::commands::server::dispatch(ctx, method, params).await
                .map_err(|e| e.to_string())
        }

        // Cost tracking
        "get_project_costs" => {
            let worktree_path = params["worktree_path"].as_str().ok_or("missing worktree_path")?.to_string();
            let costs = racc_core::commands::cost::get_project_costs(worktree_path).await.map_err(|e| e.to_string())?;
            Ok(serde_json::to_value(costs).unwrap())
        }
        "get_global_costs" => {
            let costs = racc_core::commands::cost::get_global_costs().await.map_err(|e| e.to_string())?;
            Ok(serde_json::to_value(costs).unwrap())
        }

        // File operations
        "read_file" | "search_files" => {
            racc_core::commands::file::dispatch(ctx, method, params).await
                .map_err(|e| e.to_string())
        }

        // Insights
        "record_session_events" | "get_insights" | "update_insight_status"
        | "run_batch_analysis" | "save_insight" => {
            racc_core::commands::insights::dispatch(ctx, method, params).await
                .map_err(|e| e.to_string())
        }

        // Shell (browser fallback — just no-op, frontend handles with window.open)
        "open_url" => Ok(Value::Null),

        // Task image operations
        "save_task_image" | "delete_task_image" | "rename_task_image"
        | "update_task_images" | "copy_file_to_task_images" => {
            racc_core::commands::task::dispatch_image(ctx, method, params).await
                .map_err(|e| e.to_string())
        }

        _ => Err(format!("Unknown method: {}", method)),
    }
}
```

- [ ] **Step 2: Verify it compiles**

Run: `cd /home/devuser/racc/src-tauri && cargo check -p racc-server 2>&1 | head -30`

- [ ] **Step 3: Build the React frontend and test end-to-end**

```bash
cd /home/devuser/racc && bun run build
cd src-tauri && cargo run --bin racc-server
# In another terminal: open http://localhost:9399 in browser
```

- [ ] **Step 4: Commit**

```bash
git add src-tauri/racc-server/src/ws.rs
git commit -m "feat: implement WebSocket handler for racc-server with full dispatch"
```

---

## Chunk 3: Frontend Transport Abstraction

### Task 10: Create RaccTransport interface and implementations

**Files:**
- Create: `src/services/transport.ts`

- [ ] **Step 1: Create src/services/transport.ts**

```typescript
import { invoke } from "@tauri-apps/api/core";
import { listen, UnlistenFn } from "@tauri-apps/api/event";

export interface RaccTransport {
  call(method: string, params?: Record<string, unknown>): Promise<any>;
  on(event: string, handler: (data: any) => void): () => void;
  onTerminalData(sessionId: number, handler: (data: Uint8Array) => void): () => void;
  isLocal(): boolean;
}

// --- Tauri Transport (desktop app) ---

class TauriTransport implements RaccTransport {
  call(method: string, params?: Record<string, unknown>): Promise<any> {
    return invoke(method, params ?? {});
  }

  on(event: string, handler: (data: any) => void): () => void {
    let unlisten: UnlistenFn | null = null;
    listen(event, (e) => handler(e.payload)).then((fn) => { unlisten = fn; });
    return () => { unlisten?.(); };
  }

  onTerminalData(sessionId: number, handler: (data: Uint8Array) => void): () => void {
    let unlisten: UnlistenFn | null = null;
    listen<{ session_id: number; data: number[] }>("transport:data", (e) => {
      if (e.payload.session_id === sessionId) {
        handler(new Uint8Array(e.payload.data));
      }
    }).then((fn) => { unlisten = fn; });
    return () => { unlisten?.(); };
  }

  isLocal(): boolean { return true; }
}

// --- WebSocket Transport (browser) ---

class WebSocketTransport implements RaccTransport {
  private ws: WebSocket;
  private pending = new Map<string, { resolve: (v: any) => void; reject: (e: any) => void }>();
  private eventHandlers = new Map<string, Set<(data: any) => void>>();
  private terminalHandlers = new Map<number, Set<(data: Uint8Array) => void>>();
  private nextId = 1;
  private ready: Promise<void>;

  constructor(host: string) {
    const protocol = window.location.protocol === "https:" ? "wss:" : "ws:";
    this.ws = new WebSocket(`${protocol}//${host}/ws`);
    this.ws.binaryType = "arraybuffer";

    this.ready = new Promise((resolve) => {
      this.ws.onopen = () => resolve();
    });

    this.ws.onmessage = (event) => {
      if (event.data instanceof ArrayBuffer) {
        // Binary frame: first 8 bytes = session_id (i64 LE)
        const view = new DataView(event.data);
        const sessionId = Number(view.getBigInt64(0, true));
        const data = new Uint8Array(event.data, 8);
        const handlers = this.terminalHandlers.get(sessionId);
        if (handlers) {
          handlers.forEach((h) => h(data));
        }
        return;
      }

      const msg = JSON.parse(event.data);
      if (msg.id) {
        // Response to a call
        const pending = this.pending.get(msg.id);
        if (pending) {
          this.pending.delete(msg.id);
          if (msg.error) pending.reject(new Error(msg.error));
          else pending.resolve(msg.result);
        }
      } else if (msg.event) {
        // Push event
        const handlers = this.eventHandlers.get(msg.event);
        if (handlers) {
          handlers.forEach((h) => h(msg.data));
        }
        // Also fire a wildcard for any-event listeners
        const allHandlers = this.eventHandlers.get("*");
        if (allHandlers) {
          allHandlers.forEach((h) => h(msg));
        }
      }
    };
  }

  async call(method: string, params?: Record<string, unknown>): Promise<any> {
    await this.ready;
    const id = String(this.nextId++);
    return new Promise((resolve, reject) => {
      this.pending.set(id, { resolve, reject });
      this.ws.send(JSON.stringify({ id, method, params: params ?? {} }));
    });
  }

  on(event: string, handler: (data: any) => void): () => void {
    if (!this.eventHandlers.has(event)) {
      this.eventHandlers.set(event, new Set());
    }
    this.eventHandlers.get(event)!.add(handler);
    return () => {
      this.eventHandlers.get(event)?.delete(handler);
    };
  }

  onTerminalData(sessionId: number, handler: (data: Uint8Array) => void): () => void {
    if (!this.terminalHandlers.has(sessionId)) {
      this.terminalHandlers.set(sessionId, new Set());
    }
    this.terminalHandlers.get(sessionId)!.add(handler);
    return () => {
      this.terminalHandlers.get(sessionId)?.delete(handler);
    };
  }

  isLocal(): boolean { return false; }
}

// --- Auto-detection ---

function createTransport(): RaccTransport {
  if (typeof window !== "undefined" && (window as any).__TAURI_INTERNALS__) {
    return new TauriTransport();
  }
  return new WebSocketTransport(window.location.host);
}

export const transport = createTransport();
```

- [ ] **Step 2: Verify TypeScript compiles**

Run: `cd /home/devuser/racc && bunx tsc --noEmit 2>&1 | head -20`

- [ ] **Step 3: Commit**

```bash
git add src/services/transport.ts
git commit -m "feat: add RaccTransport abstraction with Tauri and WebSocket implementations"
```

---

### Task 11: Migrate stores to use transport abstraction

**Files:**
- Modify: `src/stores/sessionStore.ts`
- Modify: `src/stores/taskStore.ts`
- Modify: `src/stores/serverStore.ts`
- Modify: `src/stores/insightsStore.ts`
- Modify: `src/services/ptyManager.ts`
- Modify: `src/services/eventCapture.ts`

- [ ] **Step 1: Migrate sessionStore.ts**

Replace:
```typescript
import { invoke } from "@tauri-apps/api/core";
```
With:
```typescript
import { transport } from "../services/transport";
```

Then replace each `invoke("command_name", { ... })` with `transport.call("command_name", { ... })`.

Replace `listen(...)` calls with `transport.on(...)`.

For `sendNotification` from `@tauri-apps/plugin-notification`:
```typescript
// Add browser fallback
function notify(title: string, body: string) {
  if (transport.isLocal()) {
    import("@tauri-apps/plugin-notification").then(m => m.sendNotification({ title, body }));
  } else if ("Notification" in window) {
    new Notification(title, { body });
  }
}
```

- [ ] **Step 2: Migrate taskStore.ts**

Same pattern — replace `invoke()` with `transport.call()`.

- [ ] **Step 3: Migrate serverStore.ts**

Same pattern — replace `invoke()` with `transport.call()`.

- [ ] **Step 4: Migrate insightsStore.ts**

Replace `invoke()` with `transport.call()`. Replace `listen("insight-detected", ...)` with `transport.on(...)`.

- [ ] **Step 5: Migrate ptyManager.ts**

Replace `invoke("transport_write", ...)` and `invoke("transport_resize", ...)` with `transport.call(...)`.

- [ ] **Step 6: Migrate eventCapture.ts**

Replace `invoke("record_session_events", ...)` with `transport.call(...)`.

- [ ] **Step 7: Migrate remaining stores**

**fileViewerStore.ts:** Replace `invoke("read_file", ...)` and `invoke("search_files", ...)` with `transport.call(...)`.

**assistantStore.ts:** Replace all 7 `invoke()` calls with `transport.call(...)`. Even though assistant is out of MVP scope, these must not crash in browser mode.

**setupAgent.ts:** Replace `invoke("execute_remote_command", ...)` and `invoke("list_servers", ...)` with `transport.call(...)`.

- [ ] **Step 8: Verify TypeScript compiles**

Run: `cd /home/devuser/racc && bunx tsc --noEmit 2>&1 | head -20`

- [ ] **Step 9: Commit**

```bash
git add src/stores/ src/services/ptyManager.ts src/services/eventCapture.ts src/services/setupAgent.ts
git commit -m "feat: migrate stores and services to transport abstraction"
```

---

### Task 12: Migrate components and hooks

**Files:**
- Modify: `src/hooks/usePtyBridge.ts`
- Modify: `src/services/ptyOutputParser.ts`
- Modify: `src/App.tsx`
- Modify: `src/components/Terminal/Terminal.tsx`
- Modify: `src/components/Dashboard/StatusBar.tsx`
- Modify: `src/components/TaskBoard/TaskColumn.tsx`
- Modify: `src/components/TaskBoard/TaskInput.tsx`
- Modify: `src/components/TaskBoard/TaskBoard.tsx`
- Modify: `src/components/TaskBoard/TaskCard.tsx`
- Modify: `src/components/Sidebar/Sidebar.tsx`
- Modify: `src/components/Sidebar/ImportRepoDialog.tsx`
- Modify: `src/components/Insights/InsightActions.tsx`
- Modify: `src/components/SetupWizard/StaticGuide.tsx`

- [ ] **Step 1: Migrate usePtyBridge.ts**

Replace `listen<{ session_id: number; data: number[] }>("transport:data", ...)` with `transport.onTerminalData(sessionId, ...)`.

- [ ] **Step 2: Migrate ptyOutputParser.ts**

This is a per-session tracking service, not a simple drop-in. The `startTracking()` function opens a `listen("transport:data", ...)` per session and stores `UnlistenFn`. Refactor:
- Replace `import { listen, UnlistenFn } from "@tauri-apps/api/event"` with `import { transport } from "./transport"`
- Change `TrackedSession.unlisten` type from `UnlistenFn | null` to `(() => void) | null`
- Replace `listen("transport:data", ...)` with `transport.onTerminalData(sessionId, (data) => { ... })`
- The returned cleanup function replaces the stored `unlisten`

- [ ] **Step 3: Migrate App.tsx**

Replace `listen("menu-reset-db", ...)` with `transport.on("menu-reset-db", ...)`. Note: this menu event is desktop-only, so in browser mode this handler just never fires — acceptable for MVP.

- [ ] **Step 4: Migrate Terminal.tsx**

Replace `invoke("open_url", { url })` with:
```typescript
if (transport.isLocal()) {
  transport.call("open_url", { url });
} else {
  window.open(url, "_blank");
}
```

- [ ] **Step 5: Migrate TaskBoard components**

For `TaskBoard.tsx`, `TaskInput.tsx`, `TaskColumn.tsx` — replace `invoke()` with `transport.call()`.

For Tauri plugin imports (`@tauri-apps/plugin-dialog`, `@tauri-apps/plugin-shell`):
```typescript
// Before:
import { open as openDialog } from "@tauri-apps/plugin-dialog";

// After:
async function openDialog(options: any) {
  if (transport.isLocal()) {
    const { open } = await import("@tauri-apps/plugin-dialog");
    return open(options);
  }
  // In browser mode: use <input type="file"> fallback (or skip for MVP)
  return null;
}
```

For `TaskCard.tsx` — replace `open` from `@tauri-apps/plugin-shell` with browser `window.open()` fallback.

**CRITICAL: `convertFileSrc` migration.** `TaskInput.tsx` and `TaskCard.tsx` use `convertFileSrc` from `@tauri-apps/api/core` to display task images. In browser mode, these asset protocol URLs don't work. Replace with:
```typescript
function getImageUrl(path: string): string {
  if (transport.isLocal()) {
    // Dynamic import to avoid bundler issues
    return (window as any).__TAURI_INTERNALS__
      ? `asset://localhost/${encodeURIComponent(path)}`
      : path;
  }
  // In browser mode, serve images via HTTP from racc-server
  return `/api/file?path=${encodeURIComponent(path)}`;
}
```
Note: This requires a `/api/file` endpoint on racc-server (add to `http.rs`). Alternatively, for MVP, task images can be non-functional in browser mode — document as a known limitation.

**CRITICAL: ALL static Tauri plugin imports MUST become dynamic imports.** If a top-level `import { open } from "@tauri-apps/plugin-dialog"` is in the bundle, the browser will crash before the app loads. Convert every static import to dynamic `await import(...)` wrapped in `transport.isLocal()` checks. Files: `TaskBoard.tsx`, `TaskInput.tsx`, `TaskCard.tsx`, `Sidebar.tsx`, `ImportRepoDialog.tsx`.

- [ ] **Step 6: Migrate Sidebar, ImportRepoDialog, StatusBar, StaticGuide**

Replace plugin imports with dynamic imports. Replace `invoke()` with `transport.call()`.

For `ImportRepoDialog` — in browser mode, the file picker dialog is not available. Show a text input for repo path instead (the server has filesystem access).

For `StatusBar.tsx` — replace `invoke("get_global_costs", ...)` with `transport.call(...)`.

For `StaticGuide.tsx` — replace `invoke("execute_remote_command", ...)` with `transport.call(...)`.

- [ ] **Step 7: Migrate InsightActions.tsx**

Replace `invoke("append_to_file", ...)` with `transport.call(...)`.

- [ ] **Step 8: Verify TypeScript compiles**

Run: `cd /home/devuser/racc && bunx tsc --noEmit 2>&1 | head -20`

- [ ] **Step 9: Test in Tauri dev mode**

Run: `cd /home/devuser/racc && bun tauri dev`

Verify all features still work: sessions, terminal, task board.

- [ ] **Step 10: Commit**

```bash
git add src/hooks/ src/services/ptyOutputParser.ts src/App.tsx src/components/
git commit -m "feat: migrate all components and hooks to transport abstraction"
```

---

## Chunk 4: Integration and Consolidation

### Task 13: Consolidate ws_server.rs to use racc-core

**Files:**
- Modify: `src-tauri/src/ws_server.rs`

The existing `ws_server.rs` reimplements business logic with raw SQL. Replace handler functions with calls to `racc_core::commands::*`.

- [ ] **Step 1: Replace handler functions**

For each handler (`handle_create_task`, `handle_create_session`, `handle_stop_session`, etc.), replace the raw SQL with calls to `racc_core` functions. The dispatch function should follow the same pattern as `racc-server/src/ws.rs`.

Keep the existing WebSocket infrastructure (tokio-tungstenite on `127.0.0.1:9399`) — the Tauri app still uses it for local remote API access.

- [ ] **Step 2: Remove duplicated event emission**

The existing `emit_event()` function (line 53-57) emits to both broadcast and `app_handle.emit()`. Since `racc_core` commands now emit through `EventBus`, the WS server just needs to forward broadcast events to WS clients (already done by the event broadcaster task at line 119).

Remove the extra `app.emit()` calls from individual handlers since `racc_core` handles this.

- [ ] **Step 3: Verify it compiles**

Run: `cd /home/devuser/racc/src-tauri && cargo check 2>&1 | head -20`

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/ws_server.rs
git commit -m "refactor: consolidate ws_server.rs to call racc-core instead of raw SQL"
```

---

### Task 14: End-to-end testing

- [ ] **Step 1: Test Tauri desktop app**

```bash
cd /home/devuser/racc && bun tauri dev
```

Verify:
- Create a session → terminal works
- Task board → create/move/delete tasks
- Stop session → status updates
- PR URL detection works

- [ ] **Step 2: Test racc-server in browser**

```bash
cd /home/devuser/racc && bun run build
cd src-tauri && cargo run --bin racc-server
# Open http://localhost:9399 in browser
```

Verify:
- Page loads with full UI
- WebSocket connects (check browser devtools Network tab)
- Create a session → terminal output streams
- Task board works
- Type in terminal → input reaches PTY
- Stop session → status updates
- Refresh page → reconnects, restores state via `sync` + `transport_get_buffer`

- [ ] **Step 3: Test Tailscale access (if available)**

From another device on the Tailscale network:
```
http://<tailscale-hostname>:9399
```

Verify same browser functionality works remotely.

- [ ] **Step 4: Commit any fixes**

```bash
git add -A
git commit -m "fix: address issues found during end-to-end testing"
```

---

### Task 15: Update documentation

**Files:**
- Modify: `wiki/Technical-Architecture.md`
- Modify: `wiki/Roadmap.md`
- Modify: `CLAUDE.md`
- Modify: `README.md`

- [ ] **Step 1: Update CLAUDE.md commands section**

Add:
```
cargo build --bin racc-server   # Build headless server
./racc-server                    # Run on :9399 (RACC_PORT, RACC_DB_PATH, RACC_DIST_PATH env vars)
```

- [ ] **Step 2: Update wiki/Technical-Architecture.md**

Add a section on the three-crate architecture (racc-core, racc-server, racc-tauri) and the frontend transport abstraction.

- [ ] **Step 3: Update README.md**

Add a "Headless Server" section under Quick Start:
```markdown
### Headless Server (browser access)

```bash
bun run build
cd src-tauri && cargo build --bin racc-server
./target/release/racc-server
# Open http://localhost:9399 or http://<tailscale-host>:9399
```

- [ ] **Step 4: Commit**

```bash
git add wiki/ CLAUDE.md README.md
git commit -m "docs: update architecture docs for headless server"
```
