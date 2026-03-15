# Remote Server Connection Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add remote server support to Racc — SSH-based connections, Transport abstraction layer, AI-driven setup wizard, and remote tmux-based agent sessions that are seamless with local sessions.

**Architecture:** Introduce a `Transport` trait in Rust with `LocalPtyTransport` and `SshTmuxTransport` implementations. Frontend migrates from direct `tauri-plugin-pty` calls to unified Tauri transport commands. New `SshManager` manages SSH connections via `russh`. Setup agent uses `@mariozechner/pi-agent-core` with fallback to static guide.

**Tech Stack:** Rust (`russh`, `ssh2-config`, `rusqlite`), TypeScript/React, `@mariozechner/pi-agent-core`, `@mariozechner/pi-ai`, xterm.js, Tauri 2.x

**Spec:** `docs/superpowers/specs/2026-03-15-remote-server-connection-design.md`

---

## Chunk 1: Transport Abstraction Layer (Phase 1)

Refactor existing PTY management from frontend-driven to Rust-side Transport trait. After this chunk, local sessions work identically but through the new architecture.

### File Structure

| Action | Path | Responsibility |
|--------|------|----------------|
| Create | `src-tauri/src/transport/mod.rs` | Transport trait + TransportError + RingBuffer |
| Create | `src-tauri/src/transport/local_pty.rs` | LocalPtyTransport wrapping tauri-plugin-pty |
| Create | `src-tauri/src/transport/manager.rs` | TransportManager: HashMap<i64, Box<dyn Transport>> + buffers |
| Create | `src-tauri/src/commands/transport.rs` | Tauri commands: transport_write, transport_resize, transport_get_buffer |
| Modify | `src-tauri/src/lib.rs` | Register transport commands + TransportManager state |
| Modify | `src-tauri/src/commands/mod.rs` | Add `pub mod transport;` |
| Modify | `src-tauri/src/commands/session.rs` | create_session spawns transport instead of returning for frontend PTY |
| Rewrite | `src/services/ptyManager.ts` | Replace with thin wrapper calling transport commands |
| Modify | `src/services/ptyOutputParser.ts` | Switch from ptyManager.subscribe to transport:data events |
| Modify | `src/hooks/usePtyBridge.ts` | Switch from ptyManager subscribe to Tauri event listener |
| Modify | `src/stores/sessionStore.ts` | Remove direct ptyManager.spawnPty calls, use transport |
| Modify | `src/types/session.ts` | Add server_id field |

---

### Task 1: Define Transport trait and RingBuffer

**Files:**
- Create: `src-tauri/src/transport/mod.rs`

- [ ] **Step 1: Create transport module with trait and types**

```rust
// src-tauri/src/transport/mod.rs
pub mod local_pty;
pub mod manager;

use async_trait::async_trait;
use std::collections::VecDeque;
use std::fmt;

#[derive(Debug)]
pub enum TransportError {
    NotFound(String),
    IoError(String),
    Closed,
}

impl fmt::Display for TransportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TransportError::NotFound(msg) => write!(f, "Transport not found: {}", msg),
            TransportError::IoError(msg) => write!(f, "I/O error: {}", msg),
            TransportError::Closed => write!(f, "Transport closed"),
        }
    }
}

impl From<TransportError> for String {
    fn from(e: TransportError) -> String {
        e.to_string()
    }
}

#[async_trait]
pub trait Transport: Send + Sync {
    /// Write data to the transport (PTY stdin or SSH channel stdin).
    async fn write(&self, data: &[u8]) -> Result<(), TransportError>;

    /// Resize the terminal dimensions.
    async fn resize(&self, cols: u16, rows: u16) -> Result<(), TransportError>;

    /// Close the transport and clean up resources.
    async fn close(&self) -> Result<(), TransportError>;

    /// Check if the transport is still alive.
    fn is_alive(&self) -> bool;
}

/// Ring buffer for terminal output. Drops oldest chunks when exceeding max size.
/// Uses VecDeque for O(1) front removal on the hot output path.
pub struct RingBuffer {
    chunks: VecDeque<Vec<u8>>,
    total_size: usize,
    max_size: usize,
}

impl RingBuffer {
    pub fn new(max_size: usize) -> Self {
        Self {
            chunks: VecDeque::new(),
            total_size: 0,
            max_size,
        }
    }

    pub fn push(&mut self, data: Vec<u8>) {
        self.total_size += data.len();
        self.chunks.push_back(data);
        while self.total_size > self.max_size {
            if let Some(removed) = self.chunks.pop_front() {
                self.total_size -= removed.len();
            } else {
                break;
            }
        }
    }

    pub fn get_all(&self) -> Vec<u8> {
        self.chunks.iter().flat_map(|c| c.iter()).copied().collect()
    }

    pub fn clear(&mut self) {
        self.chunks.clear();
        self.total_size = 0;
    }
}
```

- [ ] **Step 2: Add `async-trait` to Cargo.toml**

Add to `src-tauri/Cargo.toml` under `[dependencies]`:
```toml
async-trait = "0.1"
```

- [ ] **Step 3: Register transport module**

Add to `src-tauri/src/main.rs` or wherever modules are declared:
```rust
mod transport;
```

Check existing module structure — if modules are declared in `lib.rs`, add it there.

- [ ] **Step 4: Verify it compiles**

Run: `cd src-tauri && cargo check`
Expected: Compiles with no errors (unused warnings OK).

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/transport/mod.rs src-tauri/Cargo.toml
git commit -m "feat: define Transport trait and RingBuffer"
```

---

### Task 2: Implement LocalPtyTransport

**Files:**
- Create: `src-tauri/src/transport/local_pty.rs`

- [ ] **Step 1: Replace tauri-plugin-pty with portable-pty**

`tauri-plugin-pty` is a frontend-driven PTY plugin — it spawns PTYs from JavaScript and provides no Rust-side API for spawning. Since we need Rust-side PTY control for the Transport abstraction, we must switch to `portable-pty` (the same underlying crate that `tauri-plugin-pty` wraps).

1. Remove `tauri-plugin-pty` from `src-tauri/Cargo.toml`
2. Remove `.plugin(tauri_plugin_pty::init())` from `src-tauri/src/lib.rs` (around line 18)
3. Remove `tauri-pty` from `package.json` frontend dependencies
4. Add `portable-pty` to `src-tauri/Cargo.toml`:

```toml
portable-pty = "0.8"
```

5. Verify: `cd src-tauri && cargo check` (will have errors from removed plugin — that's expected, we're replacing it)

- [ ] **Step 2: Implement LocalPtyTransport**

```rust
// src-tauri/src/transport/local_pty.rs
use super::{Transport, TransportError};
use async_trait::async_trait;
use std::sync::Arc;
use tokio::sync::Mutex;
use tauri::AppHandle;

/// LocalPtyTransport wraps a native PTY process.
/// Output is pushed via a background task to:
/// 1. Tauri event emit (for real-time xterm.js rendering)
/// 2. RingBuffer in TransportManager (for session-switch replay)
pub struct LocalPtyTransport {
    session_id: i64,
    // The actual PTY handle — exact type depends on tauri-plugin-pty's Rust API
    // or we may use portable-pty directly
    pty_writer: Arc<Mutex<Box<dyn std::io::Write + Send>>>,
    pty_master: Arc<Mutex<Option<portable_pty::MasterPty>>>,
    alive: Arc<std::sync::atomic::AtomicBool>,
}

impl LocalPtyTransport {
    /// Spawn a new local PTY process.
    /// `cwd` — working directory
    /// `cmd` — shell command to run (e.g., "/bin/zsh --login")
    /// `cols`, `rows` — initial terminal dimensions
    /// `app` — Tauri app handle for emitting events
    /// `buffer_tx` — channel sender to push output to TransportManager's RingBuffer
    pub async fn spawn(
        session_id: i64,
        cwd: &str,
        cmd: &str,
        cols: u16,
        rows: u16,
        app: AppHandle,
        buffer_tx: tokio::sync::mpsc::UnboundedSender<(i64, Vec<u8>)>,
    ) -> Result<Self, TransportError> {
        // Use portable-pty crate to spawn PTY from Rust side
        // 1. Create PtySystem
        // 2. Open PTY pair with cols/rows
        // 3. Spawn child process with cmd in cwd
        // 4. Start background read task that:
        //    a. Reads from PTY master
        //    b. Emits "transport:data" event with { session_id, data }
        //    c. Sends data to buffer_tx for RingBuffer storage
        // 5. Return Self with write handle

        // Using portable-pty (tauri-plugin-pty has been removed)
        use portable_pty::{CommandBuilder, PtySize, native_pty_system};

        let pty_system = native_pty_system();
        let pair = pty_system.openpty(PtySize { rows, cols, pixel_width: 0, pixel_height: 0 })
            .map_err(|e| TransportError::IoError(e.to_string()))?;

        let mut cmd = CommandBuilder::new(cmd);
        cmd.cwd(cwd);
        cmd.env("TERM", "xterm-256color");
        let _child = pair.slave.spawn_command(cmd)
            .map_err(|e| TransportError::IoError(e.to_string()))?;
        drop(pair.slave); // Close slave side in parent

        let writer = pair.master.try_clone_writer()
            .map_err(|e| TransportError::IoError(e.to_string()))?;
        let mut reader = pair.master.try_clone_reader()
            .map_err(|e| TransportError::IoError(e.to_string()))?;

        let alive = Arc::new(std::sync::atomic::AtomicBool::new(true));
        let alive_clone = alive.clone();
        let sid = session_id;

        // Background read task: PTY stdout → event emit + ring buffer
        tokio::task::spawn_blocking(move || {
            let mut buf = [0u8; 4096];
            loop {
                if !alive_clone.load(std::sync::atomic::Ordering::SeqCst) { break; }
                match reader.read(&mut buf) {
                    Ok(0) => { alive_clone.store(false, std::sync::atomic::Ordering::SeqCst); break; }
                    Ok(n) => {
                        let data = buf[..n].to_vec();
                        let _ = app.emit("transport:data", serde_json::json!({ "session_id": sid, "data": &data }));
                        let _ = buffer_tx.send((sid, data));
                    }
                    Err(_) => { alive_clone.store(false, std::sync::atomic::Ordering::SeqCst); break; }
                }
            }
        });

        Ok(Self {
            session_id,
            pty_writer: Arc::new(Mutex::new(Box::new(writer))),
            pty_master: Arc::new(Mutex::new(Some(pair.master))),
            alive,
        })
    }
}

#[async_trait]
impl Transport for LocalPtyTransport {
    async fn write(&self, data: &[u8]) -> Result<(), TransportError> {
        let mut writer = self.pty_writer.lock().await;
        writer.write_all(data).map_err(|e| TransportError::IoError(e.to_string()))?;
        Ok(())
    }

    async fn resize(&self, cols: u16, rows: u16) -> Result<(), TransportError> {
        let master = self.pty_master.lock().await;
        if let Some(ref master) = *master {
            master.resize(portable_pty::PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            }).map_err(|e| TransportError::IoError(e.to_string()))?;
        }
        Ok(())
    }

    async fn close(&self) -> Result<(), TransportError> {
        self.alive.store(false, std::sync::atomic::Ordering::SeqCst);
        // Drop the master to signal EOF to child
        let mut master = self.pty_master.lock().await;
        *master = None;
        Ok(())
    }

    fn is_alive(&self) -> bool {
        self.alive.load(std::sync::atomic::Ordering::SeqCst)
    }
}
```

**Note:** `portable-pty` was added in Step 1 when we removed `tauri-plugin-pty`. The `spawn()` method now uses `portable-pty` directly. The event name `transport:data` is emitted globally via `app.emit()` — the frontend filters by `session_id` in the payload.

- [ ] **Step 3: Verify it compiles**

Run: `cd src-tauri && cargo check`
Expected: Compiles (with `todo!()` in spawn — that's OK for now).

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/transport/local_pty.rs src-tauri/Cargo.toml
git commit -m "feat: implement LocalPtyTransport skeleton"
```

---

### Task 3: Implement TransportManager

**Files:**
- Create: `src-tauri/src/transport/manager.rs`

- [ ] **Step 1: Implement TransportManager**

```rust
// src-tauri/src/transport/manager.rs
use super::{Transport, TransportError, RingBuffer};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

const MAX_BUFFER_SIZE: usize = 1_048_576; // 1MB per session

pub struct TransportManager {
    transports: Arc<Mutex<HashMap<i64, Box<dyn Transport>>>>,
    buffers: Arc<Mutex<HashMap<i64, RingBuffer>>>,
    buffer_tx: tokio::sync::mpsc::UnboundedSender<(i64, Vec<u8>)>,
    buffer_rx: Arc<Mutex<Option<tokio::sync::mpsc::UnboundedReceiver<(i64, Vec<u8>)>>>>,
}

impl TransportManager {
    pub fn new() -> Self {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        Self {
            transports: Arc::new(Mutex::new(HashMap::new())),
            buffers: Arc::new(Mutex::new(HashMap::new())),
            buffer_tx: tx,
            buffer_rx: Arc::new(Mutex::new(Some(rx))),
        }
    }

    /// Start the buffer aggregation task. Call once during app setup.
    pub fn start_buffer_task(&self) {
        let buffers = self.buffers.clone();
        let rx = self.buffer_rx.clone();
        tokio::spawn(async move {
            let mut rx = rx.lock().await.take().expect("buffer task already started");
            while let Some((session_id, data)) = rx.recv().await {
                let mut bufs = buffers.lock().await;
                if let Some(buf) = bufs.get_mut(&session_id) {
                    buf.push(data);
                }
            }
        });
    }

    pub fn buffer_sender(&self) -> tokio::sync::mpsc::UnboundedSender<(i64, Vec<u8>)> {
        self.buffer_tx.clone()
    }

    pub async fn insert(&self, session_id: i64, transport: Box<dyn Transport>) {
        self.buffers.lock().await.insert(session_id, RingBuffer::new(MAX_BUFFER_SIZE));
        self.transports.lock().await.insert(session_id, transport);
    }

    pub async fn write(&self, session_id: i64, data: &[u8]) -> Result<(), TransportError> {
        let transports = self.transports.lock().await;
        let transport = transports.get(&session_id)
            .ok_or_else(|| TransportError::NotFound(format!("session {}", session_id)))?;
        transport.write(data).await
    }

    pub async fn resize(&self, session_id: i64, cols: u16, rows: u16) -> Result<(), TransportError> {
        let transports = self.transports.lock().await;
        let transport = transports.get(&session_id)
            .ok_or_else(|| TransportError::NotFound(format!("session {}", session_id)))?;
        transport.resize(cols, rows).await
    }

    pub async fn get_buffer(&self, session_id: i64) -> Option<Vec<u8>> {
        let buffers = self.buffers.lock().await;
        buffers.get(&session_id).map(|b| b.get_all())
    }

    pub async fn remove(&self, session_id: i64) -> Result<(), TransportError> {
        if let Some(transport) = self.transports.lock().await.remove(&session_id) {
            transport.close().await?;
        }
        self.buffers.lock().await.remove(&session_id);
        Ok(())
    }

    pub async fn is_alive(&self, session_id: i64) -> bool {
        let transports = self.transports.lock().await;
        transports.get(&session_id).map_or(false, |t| t.is_alive())
    }
}
```

- [ ] **Step 2: Verify it compiles**

Run: `cd src-tauri && cargo check`
Expected: Compiles with no errors.

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/transport/manager.rs
git commit -m "feat: implement TransportManager with ring buffer aggregation"
```

---

### Task 4: Create Tauri transport commands

**Files:**
- Create: `src-tauri/src/commands/transport.rs`
- Modify: `src-tauri/src/commands/mod.rs`

- [ ] **Step 1: Create transport commands**

```rust
// src-tauri/src/commands/transport.rs
use crate::transport::manager::TransportManager;
use tauri::State;

#[tauri::command]
pub async fn transport_write(
    session_id: i64,
    data: Vec<u8>,
    transport_manager: State<'_, TransportManager>,
) -> Result<(), String> {
    transport_manager.write(session_id, &data).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn transport_resize(
    session_id: i64,
    cols: u16,
    rows: u16,
    transport_manager: State<'_, TransportManager>,
) -> Result<(), String> {
    transport_manager.resize(session_id, cols, rows).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn transport_get_buffer(
    session_id: i64,
    transport_manager: State<'_, TransportManager>,
) -> Result<Vec<u8>, String> {
    transport_manager.get_buffer(session_id).await
        .ok_or_else(|| format!("No buffer for session {}", session_id))
}
```

- [ ] **Step 2: Register module in commands/mod.rs**

Read `src-tauri/src/commands/mod.rs` and add:
```rust
pub mod transport;
```

- [ ] **Step 3: Register commands and state in lib.rs**

Modify `src-tauri/src/lib.rs`:

1. Add `TransportManager` to managed state (near line 20-22 where db pool is added):
```rust
let transport_manager = transport::manager::TransportManager::new();
transport_manager.start_buffer_task();
// ...
.manage(transport_manager)
```

2. Add transport commands to the invoke handler (near lines 64-105):
```rust
commands::transport::transport_write,
commands::transport::transport_resize,
commands::transport::transport_get_buffer,
```

- [ ] **Step 4: Verify it compiles**

Run: `cd src-tauri && cargo check`
Expected: Compiles with no errors.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/commands/transport.rs src-tauri/src/commands/mod.rs src-tauri/src/lib.rs
git commit -m "feat: add Tauri transport commands (write, resize, get_buffer)"
```

---

### Task 5: Wire create_session to spawn LocalPtyTransport

**Files:**
- Modify: `src-tauri/src/commands/session.rs` (lines 206-308)

- [ ] **Step 1: Read current create_session implementation**

Read `src-tauri/src/commands/session.rs` lines 206-308 to understand the full flow. Currently it:
1. Creates DB record
2. Creates git worktree if requested
3. Emits SessionStatusChanged event
4. Returns Session struct

The PTY is spawned by the frontend after receiving the Session. We need to move PTY spawning here.

- [ ] **Step 2: Add new parameters to create_session**

The current `create_session` signature only accepts `repo_id`, `use_worktree`, and `branch`. Add the following new parameters:

```rust
pub async fn create_session(
    repo_id: i64,
    use_worktree: bool,
    branch: Option<String>,
    agent: Option<String>,          // NEW — defaults to "claude-code"
    task_description: Option<String>, // NEW — task text to send to agent
    server_id: Option<String>,      // NEW — null = local session
    transport_manager: State<'_, TransportManager>,  // NEW
    app: AppHandle,                 // NEW
    db: State<'_, DbPool>,
    event_sender: State<'_, EventSender>,
) -> Result<Session, String> {
    let agent = agent.unwrap_or_else(|| "claude-code".to_string());
    let task_description = task_description.unwrap_or_default();
    // ... rest of function
}
```

Update all frontend callers (`sessionStore.ts`, `ws_server.rs`) to pass the new params.

- [ ] **Step 3: Add transport_manager spawn logic**

After the DB insert and before returning Session, add transport spawning:

```rust
// Add to create_session params:
transport_manager: State<'_, TransportManager>,
app: AppHandle,

// After DB insert and before returning Session, add:
// Only spawn transport for local sessions (server_id is None)
if server_id.is_none() {
    let cwd = worktree_path.as_deref().unwrap_or(&repo_path);
    let agent_cmd = build_agent_command(&agent, &task_description, cwd);
    let transport = LocalPtyTransport::spawn(
        session_id,
        cwd,
        "/bin/zsh",
        80, 24,  // default size, frontend will resize
        app.clone(),
        transport_manager.buffer_sender(),
    ).await.map_err(|e| e.to_string())?;
    transport_manager.insert(session_id, Box::new(transport)).await;

    // Send agent command after short delay
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    transport_manager.write(session_id, agent_cmd.as_bytes()).await
        .map_err(|e| e.to_string())?;
}
```

- [ ] **Step 4: Add helper to build agent command**

Add near the top of session.rs:
```rust
fn build_agent_command(agent: &str, task: &str, cwd: &str) -> String {
    match agent {
        "claude-code" => {
            let escaped_task = task.replace('\'', "'\\''");
            format!("claude '{}'\n", escaped_task)
        }
        "aider" => format!("aider\n"),
        "codex" => format!("codex '{}'\n", task.replace('\'', "'\\''")),
        _ => format!("{}\n", agent),
    }
}
```

- [ ] **Step 5: Add server_id to Session struct and DB insert**

Modify Session struct (around line 37):
```rust
pub struct Session {
    // ... existing fields ...
    pub server_id: Option<String>,
}
```

Modify DB insert in create_session to include server_id column.

- [ ] **Step 6: Update stop_session to close transport**

Modify `stop_session` (around line 311) to also close the transport:
```rust
// Add transport_manager param
transport_manager: State<'_, TransportManager>,

// Before updating DB status:
let _ = transport_manager.remove(session_id).await;
```

- [ ] **Step 7: Verify it compiles**

Run: `cd src-tauri && cargo check`
Expected: Compiles. May need to adjust imports and exact API calls.

- [ ] **Step 8: Commit**

```bash
git add src-tauri/src/commands/session.rs
git commit -m "feat: wire create_session to spawn LocalPtyTransport"
```

---

### Task 6: Migrate frontend to transport commands

**Files:**
- Rewrite: `src/services/ptyManager.ts`
- Modify: `src/hooks/usePtyBridge.ts`
- Modify: `src/stores/sessionStore.ts`
- Modify: `src/types/session.ts`

- [ ] **Step 1: Update session types**

Add `server_id` to `src/types/session.ts`:
```typescript
export interface Session {
  // ... existing fields ...
  server_id: string | null;
}
```

- [ ] **Step 2: Rewrite ptyManager.ts as thin transport wrapper**

```typescript
// src/services/ptyManager.ts
import { invoke } from "@tauri-apps/api/core";

// Thin wrapper over Rust-side TransportManager.
// Buffer management and PTY spawning are now handled in Rust.

export const ptyManager = {
  async write(sessionId: number, data: string): Promise<void> {
    const encoder = new TextEncoder();
    await invoke("transport_write", {
      sessionId,
      data: Array.from(encoder.encode(data)),
    });
  },

  async resize(sessionId: number, cols: number, rows: number): Promise<void> {
    await invoke("transport_resize", { sessionId, cols, rows });
  },

  async getBuffer(sessionId: number): Promise<Uint8Array> {
    const data = await invoke<number[]>("transport_get_buffer", { sessionId });
    return new Uint8Array(data);
  },

  // kill is now handled by stop_session on Rust side
  // spawnPty is now handled by create_session on Rust side
};
```

- [ ] **Step 3: Update usePtyBridge.ts**

Modify `src/hooks/usePtyBridge.ts` to:
1. Replace `ptyManager.subscribe()` with Tauri event listener for `transport:data`
2. Replace buffer replay with `ptyManager.getBuffer()` call
3. Replace `ptyManager.writePty()` with `ptyManager.write()`
4. Replace `ptyManager.resizePty()` with `ptyManager.resize()`

```typescript
import { useEffect } from "react";
import { listen } from "@tauri-apps/api/event";
import { ptyManager } from "@/services/ptyManager";
import type { Terminal } from "@xterm/xterm";

export function usePtyBridge(
  terminal: Terminal | null,
  sessionId: number | null
) {
  // Output: listen for transport:data events
  useEffect(() => {
    if (!terminal || sessionId == null) return;

    // Replay buffer on session switch
    ptyManager.getBuffer(sessionId).then((buffer) => {
      if (buffer.length > 0) {
        terminal.write(buffer);
      }
    });

    // Listen for live output
    const unlisten = listen<{ session_id: number; data: number[] }>(
      "transport:data",
      (event) => {
        if (event.payload.session_id === sessionId) {
          terminal.write(new Uint8Array(event.payload.data));
        }
      }
    );

    return () => {
      unlisten.then((fn) => fn());
    };
  }, [terminal, sessionId]);

  // Input: forward keyboard to transport
  useEffect(() => {
    if (!terminal || sessionId == null) return;

    const onData = terminal.onData((data) => {
      ptyManager.write(sessionId, data);
    });

    // Preserve Shift+Enter kitty protocol handling.
    // IMPORTANT: Must use attachCustomKeyEventHandler (fires BEFORE xterm processes
    // the key) not onKey (fires AFTER). Using onKey would double-send the keystroke.
    const customKeyHandler = terminal.attachCustomKeyEventHandler((e) => {
      if (e.type === "keydown" && e.key === "Enter" && e.shiftKey) {
        e.preventDefault();
        ptyManager.write(sessionId, "\x1b[13;2u");
        return false; // prevent xterm default handling
      }
      // Shift+single-char: bypass IME mode-switching
      if (e.type === "keydown" && e.shiftKey && e.key.length === 1) {
        return false;
      }
      return true;
    });

    return () => {
      onData.dispose();
      // attachCustomKeyEventHandler doesn't return a disposable;
      // it's replaced by the next call or cleared on terminal dispose
    };
  }, [terminal, sessionId]);

  // Resize: sync terminal dimensions to transport
  useEffect(() => {
    if (!terminal || sessionId == null) return;

    const observer = new ResizeObserver(() => {
      // FitAddon handles terminal.cols/rows update
      ptyManager.resize(sessionId, terminal.cols, terminal.rows);
    });

    observer.observe(terminal.element!);
    return () => observer.disconnect();
  }, [terminal, sessionId]);
}
```

- [ ] **Step 4: Migrate ptyOutputParser.ts to transport events**

**Critical:** `src/services/ptyOutputParser.ts` currently calls `ptyManager.subscribe()` to listen for PTY output. After the ptyManager rewrite removes `subscribe()`, this will break. Rewrite it to listen on `transport:data` Tauri events instead:

```typescript
// In ptyOutputParser.ts — replace ptyManager.subscribe() with:
import { listen } from "@tauri-apps/api/event";

export function startTracking(sessionId: number, callbacks: OutputCallbacks) {
  const unlisten = listen<{ session_id: number; data: number[] }>(
    "transport:data",
    (event) => {
      if (event.payload.session_id === sessionId) {
        const text = new TextDecoder().decode(new Uint8Array(event.payload.data));
        // ... existing parsing logic (PR URL detection, prompt detection, etc.)
      }
    }
  );
  return () => { unlisten.then((fn) => fn()); };
}
```

- [ ] **Step 5: Update sessionStore.ts**

Modify `src/stores/sessionStore.ts`:
1. Remove `ptyManager.spawnPty()` calls from `createSession()` (lines 177-206) — PTY is now spawned by Rust
2. Remove `ptyManager.killPty()` calls from `stopSession()` (lines 232-251) — transport closed by Rust
3. Remove `ptyManager.killAll()` from cleanup
4. Update `initialize()` (lines 61-141) to use the migrated `ptyOutputParser.startTracking()` which now listens on `transport:data` events
5. Pass new params (`agent`, `taskDescription`, `serverId`) to `invoke("create_session", ...)`

- [ ] **Step 6: Verify the full app builds**

Run: `bun run build && cd src-tauri && cargo check`
Expected: Both frontend and backend compile.

- [ ] **Step 7: Manual test**

Run: `bun tauri dev`
Test: Create a session, verify terminal output appears, type commands, resize window. Verify PR URL detection still works.

- [ ] **Step 8: Commit**

```bash
git add src/services/ptyManager.ts src/services/ptyOutputParser.ts src/hooks/usePtyBridge.ts src/stores/sessionStore.ts src/types/session.ts
git commit -m "feat: migrate frontend to Rust-side transport commands"
```

---

### Task 7: Database migration — add server_id to sessions

**Files:**
- Modify: `src-tauri/src/commands/db.rs`

- [ ] **Step 1: Read current migration logic**

Read `src-tauri/src/commands/db.rs` to understand the user_version pragma pattern (line 95). Check the current version number.

- [ ] **Step 2: Add migration for server_id column**

The existing `db.rs` uses a single `if version < 1` block that creates all tables and sets `PRAGMA user_version = 1`. There is no incremental migration infrastructure yet. Add the new migration **after** the `if version < 1` block closes, before `Ok(conn)`:

```rust
// After the existing `if version < 1 { ... }` block:

// Migration v1 → v2: add server_id to sessions
if version < 2 {
    conn.execute("ALTER TABLE sessions ADD COLUMN server_id TEXT", [])?;
    conn.pragma_update(None, "user_version", 2)?;
}
```

This establishes the incremental migration pattern for future schema changes. Version 2 is the next after the existing version 1.

- [ ] **Step 3: Verify it compiles and runs**

Run: `cd src-tauri && cargo check`
Expected: Compiles.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/commands/db.rs
git commit -m "feat: add server_id column to sessions table"
```

---

## Chunk 2: Server Management (Phase 2)

SSH connection management, servers table, and frontend server CRUD UI.

### File Structure

| Action | Path | Responsibility |
|--------|------|----------------|
| Create | `src-tauri/src/ssh/mod.rs` | SshManager, SshConnection, ConnectionStatus |
| Create | `src-tauri/src/ssh/config_parser.rs` | Parse ~/.ssh/config for host aliases |
| Create | `src-tauri/src/commands/server.rs` | Tauri commands: add/update/remove/list/connect/disconnect server |
| Modify | `src-tauri/src/commands/db.rs` | Create `servers` table |
| Modify | `src-tauri/src/commands/mod.rs` | Add `pub mod server;` |
| Modify | `src-tauri/src/lib.rs` | Register server commands + SshManager state |
| Modify | `src-tauri/Cargo.toml` | Add russh, ssh2-config dependencies |
| Create | `src/stores/serverStore.ts` | Zustand store for server CRUD |
| Create | `src/types/server.ts` | Server TypeScript types |
| Create | `src/components/Sidebar/ServerList.tsx` | Server list in sidebar |
| Create | `src/components/Sidebar/AddServerDialog.tsx` | Add/edit server modal |
| Modify | `src/components/Sidebar/Sidebar.tsx` | Add Servers section above sessions |

---

### Task 8: Create servers table

**Files:**
- Modify: `src-tauri/src/commands/db.rs`

- [ ] **Step 1: Add servers table creation**

Add to `init_db()` after existing table creations:
```rust
conn.execute(
    "CREATE TABLE IF NOT EXISTS servers (
        id TEXT PRIMARY KEY,
        name TEXT NOT NULL,
        host TEXT NOT NULL,
        port INTEGER DEFAULT 22,
        username TEXT NOT NULL,
        auth_method TEXT NOT NULL,
        key_path TEXT,
        ssh_config_host TEXT,
        setup_status TEXT DEFAULT 'pending',
        setup_details TEXT,
        ai_provider TEXT,
        ai_api_key TEXT,
        created_at TEXT NOT NULL,
        updated_at TEXT NOT NULL
    )",
    [],
)?;
```

Note: `ai_provider` and `ai_api_key` store the optional LLM API key for the setup agent.

- [ ] **Step 2: Verify it compiles**

Run: `cd src-tauri && cargo check`

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/commands/db.rs
git commit -m "feat: create servers table in SQLite schema"
```

---

### Task 9: Implement server CRUD commands

**Files:**
- Create: `src-tauri/src/commands/server.rs`
- Modify: `src-tauri/src/commands/mod.rs`

- [ ] **Step 1: Create Server struct and CRUD commands**

```rust
// src-tauri/src/commands/server.rs
use rusqlite::params;
use serde::{Deserialize, Serialize};
use tauri::State;
use crate::commands::db::DbPool;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Server {
    pub id: String,
    pub name: String,
    pub host: String,
    pub port: i32,
    pub username: String,
    pub auth_method: String,  // "key" | "ssh_config" | "agent"
    pub key_path: Option<String>,
    pub ssh_config_host: Option<String>,
    pub setup_status: String,
    pub setup_details: Option<String>,
    pub ai_provider: Option<String>,
    pub ai_api_key: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Deserialize)]
pub struct ServerConfig {
    pub name: String,
    pub host: String,
    pub port: Option<i32>,
    pub username: String,
    pub auth_method: String,
    pub key_path: Option<String>,
    pub ssh_config_host: Option<String>,
    pub ai_provider: Option<String>,
    pub ai_api_key: Option<String>,
}

#[tauri::command]
pub async fn add_server(
    config: ServerConfig,
    db: State<'_, DbPool>,
) -> Result<Server, String> {
    let conn = db.0.lock().map_err(|e| e.to_string())?;
    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();
    let port = config.port.unwrap_or(22);

    conn.execute(
        "INSERT INTO servers (id, name, host, port, username, auth_method, key_path, ssh_config_host, ai_provider, ai_api_key, setup_status, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, 'pending', ?11, ?11)",
        params![id, config.name, config.host, port, config.username, config.auth_method, config.key_path, config.ssh_config_host, config.ai_provider, config.ai_api_key, now],
    ).map_err(|e| e.to_string())?;

    Ok(Server {
        id, name: config.name, host: config.host, port,
        username: config.username, auth_method: config.auth_method,
        key_path: config.key_path, ssh_config_host: config.ssh_config_host,
        setup_status: "pending".to_string(), setup_details: None,
        ai_provider: config.ai_provider, ai_api_key: config.ai_api_key,
        created_at: now.clone(), updated_at: now,
    })
}

#[tauri::command]
pub async fn update_server(
    server_id: String,
    config: ServerConfig,
    db: State<'_, DbPool>,
) -> Result<Server, String> {
    let conn = db.0.lock().map_err(|e| e.to_string())?;
    let now = chrono::Utc::now().to_rfc3339();
    let port = config.port.unwrap_or(22);

    conn.execute(
        "UPDATE servers SET name=?1, host=?2, port=?3, username=?4, auth_method=?5, key_path=?6, ssh_config_host=?7, ai_provider=?8, ai_api_key=?9, updated_at=?10 WHERE id=?11",
        params![config.name, config.host, port, config.username, config.auth_method, config.key_path, config.ssh_config_host, config.ai_provider, config.ai_api_key, now, server_id],
    ).map_err(|e| e.to_string())?;

    // Re-read and return
    let server = conn.query_row(
        "SELECT id, name, host, port, username, auth_method, key_path, ssh_config_host, setup_status, setup_details, ai_provider, ai_api_key, created_at, updated_at FROM servers WHERE id=?1",
        params![server_id],
        |row| Ok(Server {
            id: row.get(0)?, name: row.get(1)?, host: row.get(2)?, port: row.get(3)?,
            username: row.get(4)?, auth_method: row.get(5)?, key_path: row.get(6)?,
            ssh_config_host: row.get(7)?, setup_status: row.get(8)?, setup_details: row.get(9)?,
            ai_provider: row.get(10)?, ai_api_key: row.get(11)?,
            created_at: row.get(12)?, updated_at: row.get(13)?,
        }),
    ).map_err(|e| e.to_string())?;

    Ok(server)
}

#[tauri::command]
pub async fn remove_server(server_id: String, db: State<'_, DbPool>) -> Result<(), String> {
    let conn = db.0.lock().map_err(|e| e.to_string())?;
    conn.execute("DELETE FROM servers WHERE id=?1", params![server_id])
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub async fn list_servers(db: State<'_, DbPool>) -> Result<Vec<Server>, String> {
    let conn = db.0.lock().map_err(|e| e.to_string())?;
    let mut stmt = conn.prepare(
        "SELECT id, name, host, port, username, auth_method, key_path, ssh_config_host, setup_status, setup_details, ai_provider, ai_api_key, created_at, updated_at FROM servers ORDER BY created_at DESC"
    ).map_err(|e| e.to_string())?;

    let servers = stmt.query_map([], |row| Ok(Server {
        id: row.get(0)?, name: row.get(1)?, host: row.get(2)?, port: row.get(3)?,
        username: row.get(4)?, auth_method: row.get(5)?, key_path: row.get(6)?,
        ssh_config_host: row.get(7)?, setup_status: row.get(8)?, setup_details: row.get(9)?,
        ai_provider: row.get(10)?, ai_api_key: row.get(11)?,
        created_at: row.get(12)?, updated_at: row.get(13)?,
    })).map_err(|e| e.to_string())?
    .collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())?;

    Ok(servers)
}
```

- [ ] **Step 2: Add uuid and chrono dependencies**

```toml
# src-tauri/Cargo.toml
uuid = { version = "1", features = ["v4"] }
chrono = { version = "0.4", features = ["serde"] }
```

- [ ] **Step 3: Register module and commands**

Add `pub mod server;` to `src-tauri/src/commands/mod.rs`.

Add commands to `src-tauri/src/lib.rs` invoke handler:
```rust
commands::server::add_server,
commands::server::update_server,
commands::server::remove_server,
commands::server::list_servers,
```

- [ ] **Step 4: Verify it compiles**

Run: `cd src-tauri && cargo check`

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/commands/server.rs src-tauri/src/commands/mod.rs src-tauri/src/lib.rs src-tauri/Cargo.toml
git commit -m "feat: implement server CRUD Tauri commands"
```

---

### Task 10: Implement SshManager

**Files:**
- Create: `src-tauri/src/ssh/mod.rs`
- Create: `src-tauri/src/ssh/config_parser.rs`
- Modify: `src-tauri/Cargo.toml`
- Modify: `src-tauri/src/lib.rs`

- [ ] **Step 1: Add russh and ssh2-config dependencies**

```toml
# src-tauri/Cargo.toml
russh = "0.46"
russh-keys = "0.46"
ssh2-config = "0.3"
```

- [ ] **Step 2: Create SSH config parser**

```rust
// src-tauri/src/ssh/config_parser.rs
use ssh2_config::{ParseRule, SshConfig};
use std::path::PathBuf;

pub struct SshHostConfig {
    pub host: String,
    pub hostname: Option<String>,
    pub port: Option<u16>,
    pub user: Option<String>,
    pub identity_file: Option<PathBuf>,
}

/// Parse ~/.ssh/config and return all Host aliases.
pub fn list_ssh_hosts() -> Result<Vec<SshHostConfig>, String> {
    let config_path = dirs::home_dir()
        .ok_or("Cannot find home directory")?
        .join(".ssh/config");

    if !config_path.exists() {
        return Ok(vec![]);
    }

    let content = std::fs::read_to_string(&config_path)
        .map_err(|e| format!("Failed to read SSH config: {}", e))?;

    let config = SshConfig::default()
        .parse_str(&content, ParseRule::ALLOW_UNKNOWN_FIELDS)
        .map_err(|e| format!("Failed to parse SSH config: {}", e))?;

    // Extract hosts from config
    // Note: exact API depends on ssh2-config version
    // This is a skeleton — adjust based on actual crate API
    todo!("Parse hosts from SshConfig")
}

/// Resolve a host alias to connection parameters.
pub fn resolve_host(alias: &str) -> Result<SshHostConfig, String> {
    let hosts = list_ssh_hosts()?;
    hosts.into_iter()
        .find(|h| h.host == alias)
        .ok_or_else(|| format!("Host '{}' not found in SSH config", alias))
}
```

- [ ] **Step 3: Create SshManager**

```rust
// src-tauri/src/ssh/mod.rs
pub mod config_parser;

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

#[derive(Debug, Clone, PartialEq)]
pub enum ConnectionStatus {
    Connected,
    Disconnected,
    Reconnecting { attempt: u32 },
}

pub struct SshConnection {
    pub client: russh::client::Handle<SshClientHandler>,
    pub status: ConnectionStatus,
    pub server_id: String,
}

/// Handler for russh client callbacks
struct SshClientHandler;

impl russh::client::Handler for SshClientHandler {
    type Error = russh::Error;

    // Implement required methods — at minimum:
    // check_server_key for host verification
    // (accept all for MVP, warn user)
}

pub struct SshManager {
    connections: Arc<Mutex<HashMap<String, SshConnection>>>,
}

impl SshManager {
    pub fn new() -> Self {
        Self {
            connections: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Connect to a server using the provided config.
    pub async fn connect(
        &self,
        server_id: &str,
        host: &str,
        port: u16,
        username: &str,
        auth_method: &str,
        key_path: Option<&str>,
    ) -> Result<(), String> {
        // 1. Load SSH key or use agent
        // 2. Connect via russh::client::connect
        // 3. Authenticate
        // 4. Store connection in HashMap
        todo!("Implement SSH connection")
    }

    /// Execute a command on a connected server.
    pub async fn exec(
        &self,
        server_id: &str,
        command: &str,
    ) -> Result<CommandOutput, String> {
        // 1. Get connection from HashMap
        // 2. Open channel
        // 3. Execute command
        // 4. Collect stdout/stderr
        // 5. Return result
        todo!("Implement SSH exec")
    }

    /// Open an interactive channel (for tmux attach).
    pub async fn open_shell(
        &self,
        server_id: &str,
        cols: u16,
        rows: u16,
    ) -> Result<russh::ChannelId, String> {
        todo!("Implement interactive shell channel")
    }

    pub async fn disconnect(&self, server_id: &str) -> Result<(), String> {
        let mut conns = self.connections.lock().await;
        if let Some(conn) = conns.remove(server_id) {
            conn.client.disconnect(russh::Disconnect::ByApplication, "", "en").await
                .map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    pub async fn is_connected(&self, server_id: &str) -> bool {
        let conns = self.connections.lock().await;
        conns.get(server_id).map_or(false, |c| c.status == ConnectionStatus::Connected)
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CommandOutput {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
}
```

- [ ] **Step 4: Register SshManager as managed state**

Modify `src-tauri/src/lib.rs`:
```rust
mod ssh;

// In setup:
let ssh_manager = ssh::SshManager::new();
// ...
.manage(ssh_manager)
```

- [ ] **Step 5: Add connect/disconnect/test/exec server commands**

Add to `src-tauri/src/commands/server.rs`:
```rust
use crate::ssh::SshManager;

#[tauri::command]
pub async fn connect_server(
    server_id: String,
    db: State<'_, DbPool>,
    ssh: State<'_, SshManager>,
) -> Result<(), String> {
    let server = get_server_by_id(&db, &server_id)?;
    ssh.connect(
        &server_id,
        &server.host,
        server.port as u16,
        &server.username,
        &server.auth_method,
        server.key_path.as_deref(),
    ).await
}

#[tauri::command]
pub async fn disconnect_server(
    server_id: String,
    ssh: State<'_, SshManager>,
) -> Result<(), String> {
    ssh.disconnect(&server_id).await
}

#[tauri::command]
pub async fn test_connection(
    server_id: String,
    db: State<'_, DbPool>,
    ssh: State<'_, SshManager>,
) -> Result<String, String> {
    connect_server(server_id.clone(), db, ssh.clone()).await?;
    let result = ssh.exec(&server_id, "echo ok").await?;
    Ok(result.stdout)
}

#[tauri::command]
pub async fn execute_remote_command(
    server_id: String,
    command: String,
    ssh: State<'_, SshManager>,
) -> Result<crate::ssh::CommandOutput, String> {
    ssh.exec(&server_id, &command).await
}
```

Register these in `lib.rs` invoke handler.

- [ ] **Step 6: Verify it compiles**

Run: `cd src-tauri && cargo check`
Expected: Compiles (with `todo!()` in SSH methods).

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src/ssh/ src-tauri/src/commands/server.rs src-tauri/src/lib.rs src-tauri/Cargo.toml
git commit -m "feat: implement SshManager skeleton with russh"
```

---

### Task 11: Add SSH config host listing command

**Files:**
- Modify: `src-tauri/src/commands/server.rs`

- [ ] **Step 1: Add command to list SSH config hosts**

```rust
#[tauri::command]
pub async fn list_ssh_config_hosts() -> Result<Vec<crate::ssh::config_parser::SshHostConfig>, String> {
    crate::ssh::config_parser::list_ssh_hosts()
}
```

Register in `lib.rs`.

- [ ] **Step 2: Commit**

```bash
git add src-tauri/src/commands/server.rs src-tauri/src/lib.rs
git commit -m "feat: add list_ssh_config_hosts command"
```

---

### Task 12: Frontend server store and types

**Files:**
- Create: `src/types/server.ts`
- Create: `src/stores/serverStore.ts`

- [ ] **Step 1: Create server types**

```typescript
// src/types/server.ts
export interface Server {
  id: string;
  name: string;
  host: string;
  port: number;
  username: string;
  auth_method: "key" | "ssh_config" | "agent";
  key_path: string | null;
  ssh_config_host: string | null;
  setup_status: "pending" | "ready" | "partial" | "error";
  setup_details: string | null;
  ai_provider: string | null;
  ai_api_key: string | null;
  created_at: string;
  updated_at: string;
}

export interface ServerConfig {
  name: string;
  host: string;
  port?: number;
  username: string;
  auth_method: "key" | "ssh_config" | "agent";
  key_path?: string;
  ssh_config_host?: string;
  ai_provider?: string;
  ai_api_key?: string;
}

export interface SshConfigHost {
  host: string;
  hostname: string | null;
  port: number | null;
  user: string | null;
}
```

- [ ] **Step 2: Create server store**

```typescript
// src/stores/serverStore.ts
import { create } from "zustand";
import { invoke } from "@tauri-apps/api/core";
import type { Server, ServerConfig, SshConfigHost } from "@/types/server";

interface ServerState {
  servers: Server[];
  loading: boolean;
  error: string | null;

  loadServers: () => Promise<void>;
  addServer: (config: ServerConfig) => Promise<Server>;
  updateServer: (serverId: string, config: ServerConfig) => Promise<Server>;
  removeServer: (serverId: string) => Promise<void>;
  connectServer: (serverId: string) => Promise<void>;
  disconnectServer: (serverId: string) => Promise<void>;
  testConnection: (serverId: string) => Promise<string>;
  listSshConfigHosts: () => Promise<SshConfigHost[]>;
}

export const useServerStore = create<ServerState>((set, get) => ({
  servers: [],
  loading: false,
  error: null,

  loadServers: async () => {
    set({ loading: true });
    try {
      const servers = await invoke<Server[]>("list_servers");
      set({ servers, loading: false });
    } catch (e) {
      set({ error: String(e), loading: false });
    }
  },

  addServer: async (config) => {
    const server = await invoke<Server>("add_server", { config });
    set((s) => ({ servers: [server, ...s.servers] }));
    return server;
  },

  updateServer: async (serverId, config) => {
    const server = await invoke<Server>("update_server", { serverId, config });
    set((s) => ({
      servers: s.servers.map((sv) => (sv.id === serverId ? server : sv)),
    }));
    return server;
  },

  removeServer: async (serverId) => {
    await invoke("remove_server", { serverId });
    set((s) => ({ servers: s.servers.filter((sv) => sv.id !== serverId) }));
  },

  connectServer: async (serverId) => {
    await invoke("connect_server", { serverId });
  },

  disconnectServer: async (serverId) => {
    await invoke("disconnect_server", { serverId });
  },

  testConnection: async (serverId) => {
    return await invoke<string>("test_connection", { serverId });
  },

  listSshConfigHosts: async () => {
    return await invoke<SshConfigHost[]>("list_ssh_config_hosts");
  },
}));
```

- [ ] **Step 3: Verify frontend builds**

Run: `bun run build`
Expected: Compiles.

- [ ] **Step 4: Commit**

```bash
git add src/types/server.ts src/stores/serverStore.ts
git commit -m "feat: add server types and Zustand store"
```

---

### Task 13: Frontend — Add Server dialog and Sidebar integration

**Files:**
- Create: `src/components/Sidebar/AddServerDialog.tsx`
- Create: `src/components/Sidebar/ServerList.tsx`
- Modify: `src/components/Sidebar/Sidebar.tsx`

- [ ] **Step 1: Create AddServerDialog component**

Create `src/components/Sidebar/AddServerDialog.tsx` with:
- Form fields: name, connection method toggle (SSH Config / Manual)
- SSH Config mode: dropdown of hosts from `listSshConfigHosts()`
- Manual mode: host, port, username, auth method, key path
- AI Setup Assistant section: provider dropdown, API key input
- Test Connection button, Add button
- Follow existing dialog patterns from the codebase (e.g., `RemoveSessionDialog`)

The component should use `useServerStore` for data operations and follow existing Tailwind token patterns (`surface-0`, `surface-1`, `accent`).

- [ ] **Step 2: Create ServerList component**

Create `src/components/Sidebar/ServerList.tsx` with:
- List of servers with connection status indicator (green dot = connected, gray = disconnected)
- Click to connect/disconnect
- Right-click or button for edit/remove
- "+" button opens AddServerDialog

Follow the existing session list pattern in `Sidebar.tsx` lines 150-261.

- [ ] **Step 3: Integrate into Sidebar**

Modify `src/components/Sidebar/Sidebar.tsx`:
- Import `ServerList`
- Add `<ServerList />` above the existing sessions section (before the repo dropdown at line 99)
- Add a horizontal divider between servers and sessions sections

- [ ] **Step 4: Load servers on app init**

Add to the app initialization flow (where `sessionStore.initialize()` is called):
```typescript
await useServerStore.getState().loadServers();
```

- [ ] **Step 5: Verify it builds and renders**

Run: `bun tauri dev`
Test: Server section visible in sidebar, Add Server dialog opens, form works.

- [ ] **Step 6: Commit**

```bash
git add src/components/Sidebar/AddServerDialog.tsx src/components/Sidebar/ServerList.tsx src/components/Sidebar/Sidebar.tsx
git commit -m "feat: add Server management UI in sidebar"
```

---

## Chunk 3: Setup Agent + Remote Sessions (Phases 3-4)

AI-driven server setup and the SshTmuxTransport for running remote agent sessions.

### File Structure

| Action | Path | Responsibility |
|--------|------|----------------|
| Create | `src/services/setupAgent.ts` | pi-agent-core setup agent with SSH tools |
| Create | `src/components/SetupWizard/SetupWizard.tsx` | Conversational setup UI (AI mode) |
| Create | `src/components/SetupWizard/StaticGuide.tsx` | Static setup guide with copy buttons (no API key) |
| Create | `src-tauri/src/transport/ssh_tmux.rs` | SshTmuxTransport implementation |
| Modify | `src-tauri/src/transport/mod.rs` | Add `pub mod ssh_tmux;` |
| Modify | `src-tauri/src/commands/session.rs` | Handle remote session creation with tmux |
| Modify | `src-tauri/src/ssh/mod.rs` | Flesh out connect/exec/open_shell |
| Modify | `src/stores/sessionStore.ts` | Remote session creation + server selection |
| Modify | `package.json` | Add pi-agent-core and pi-ai deps |

---

### Task 14: Install pi-agent-core dependencies

**Files:**
- Modify: `package.json`

- [ ] **Step 1: Install packages**

```bash
bun add @mariozechner/pi-agent-core @mariozechner/pi-ai
```

- [ ] **Step 2: Commit**

```bash
git add package.json bun.lockb
git commit -m "feat: add pi-agent-core and pi-ai dependencies"
```

---

### Task 15: Implement setup agent service

**Files:**
- Create: `src/services/setupAgent.ts`

- [ ] **Step 1: Create setup agent with remote command tools**

```typescript
// src/services/setupAgent.ts
import { Agent } from "@mariozechner/pi-agent-core";
import { getModel } from "@mariozechner/pi-ai";
import { invoke } from "@tauri-apps/api/core";

export interface SetupAgentOptions {
  serverId: string;
  provider: string;   // "anthropic" | "openai" | "openrouter"
  apiKey: string;
  onMessage: (text: string) => void;
  onCommandRun: (command: string, output: string) => void;
  onConfirmNeeded: (command: string) => Promise<boolean>;
}

export async function createSetupAgent(options: SetupAgentOptions) {
  const { serverId, provider, apiKey, onMessage, onCommandRun, onConfirmNeeded } = options;

  const runRemoteCommand = {
    name: "run_remote_command",
    description: "Execute a command on the remote server via SSH. Use requires_confirmation=true for commands that install or modify system state.",
    parameters: {
      type: "object" as const,
      properties: {
        command: { type: "string", description: "Shell command to execute" },
        requires_confirmation: { type: "boolean", description: "If true, ask user before executing" },
      },
      required: ["command"],
    },
    execute: async (args: { command: string; requires_confirmation?: boolean }) => {
      if (args.requires_confirmation) {
        const confirmed = await onConfirmNeeded(args.command);
        if (!confirmed) return "User declined to run this command.";
      }
      const result = await invoke<{ stdout: string; stderr: string; exit_code: number }>(
        "execute_remote_command",
        { serverId, command: args.command }
      );
      onCommandRun(args.command, result.stdout || result.stderr);
      return `Exit code: ${result.exit_code}\nStdout: ${result.stdout}\nStderr: ${result.stderr}`;
    },
  };

  const getServerInfo = {
    name: "get_server_info",
    description: "Get known configuration and status of this server.",
    parameters: { type: "object" as const, properties: {} },
    execute: async () => {
      const servers = await invoke<any[]>("list_servers");
      const server = servers.find((s: any) => s.id === serverId);
      return JSON.stringify(server);
    },
  };

  const agent = new Agent({
    initialState: {
      systemPrompt: `You are a server setup assistant for Racc, an Agentic IDE.
Your job is to prepare a remote server for running AI coding agents (Claude Code, Codex, etc.) via tmux.

You have SSH access to the server. Assess the environment and guide the user through setup:
1. Check OS, package manager, and available tools
2. Ensure git is installed and can access repositories (SSH keys or tokens)
3. Ensure tmux is installed
4. For each AI coding agent (claude-code, codex, etc.):
   - Check if installed
   - If installed, PRIORITIZE login/authentication setup first (e.g., "claude login" or API key configuration)
   - If not installed, offer to install with user confirmation
5. Adapt commands to the server's OS and package manager (apt, yum, brew, pacman, etc.)

Rules:
- Always use requires_confirmation=true for commands that install packages or modify system config
- Provide clear, actionable guidance for manual steps (adding SSH keys to GitHub, etc.)
- Be concise and direct
- After completing all checks, summarize the final state`,
      model: getModel(provider as any, provider === "anthropic" ? "claude-sonnet-4-20250514" : undefined),
      tools: [runRemoteCommand, getServerInfo],
    },
  });

  // Subscribe to events for streaming UI updates
  agent.subscribe((event) => {
    if (event.type === "message_update" && event.assistantMessageEvent?.type === "text_delta") {
      onMessage(event.assistantMessageEvent.delta);
    }
  });

  return agent;
}
```

- [ ] **Step 2: Verify it compiles**

Run: `bun run build`
Expected: Compiles (may need to adjust pi-agent-core API based on actual package).

- [ ] **Step 3: Commit**

```bash
git add src/services/setupAgent.ts
git commit -m "feat: implement setup agent service with pi-agent-core"
```

---

### Task 16: Implement SetupWizard and StaticGuide components

**Files:**
- Create: `src/components/SetupWizard/SetupWizard.tsx`
- Create: `src/components/SetupWizard/StaticGuide.tsx`

- [ ] **Step 1: Create StaticGuide component**

`src/components/SetupWizard/StaticGuide.tsx` — displays a checklist of commands with copy buttons:
- Detect installed components via `execute_remote_command` calls (basic checks like `which git`, `which tmux`, `which claude`)
- Show steps for missing components with copy-able commands
- "Re-check" button to re-run detection
- "Done" button to close
- Tip at bottom: "Set up an AI API key for intelligent setup assistance"

Each command block uses a `<CopyButton>` that copies the text to clipboard.

- [ ] **Step 2: Create SetupWizard component**

`src/components/SetupWizard/SetupWizard.tsx` — conversational AI setup interface:
- If server has `ai_api_key` set → render AI chat mode
- If not → render `<StaticGuide />`
- AI mode: scrollable message list + input field
- Shows agent messages, command executions (with output), confirmation prompts
- Uses `createSetupAgent()` from `src/services/setupAgent.ts`
- Prompt agent with "Check this server and help me set it up"

- [ ] **Step 3: Wire SetupWizard into AddServerDialog**

After "Test Connection" succeeds or after "Add", automatically open SetupWizard for the new server. Can be a separate dialog/panel or inline in the AddServer flow.

- [ ] **Step 4: Verify it builds**

Run: `bun run build`

- [ ] **Step 5: Manual test**

Run: `bun tauri dev`
Test with and without API key configured.

- [ ] **Step 6: Commit**

```bash
git add src/components/SetupWizard/
git commit -m "feat: add SetupWizard with AI and static guide modes"
```

---

### Task 17: Flesh out SshManager connect/exec/shell

**Files:**
- Modify: `src-tauri/src/ssh/mod.rs`

- [ ] **Step 1: Implement SSH connection**

Replace `todo!()` in `SshManager::connect()`:
1. Load SSH key from file (or use ssh-agent)
2. `russh::client::connect()` with config
3. Authenticate with key or agent
4. Store connection handle

- [ ] **Step 2: Implement command execution**

Replace `todo!()` in `SshManager::exec()`:
1. Get connection from HashMap
2. Open channel: `client.channel_open_session()`
3. `channel.exec(command)`
4. Read stdout/stderr to strings
5. Get exit status
6. Return `CommandOutput`

- [ ] **Step 3: Implement interactive shell channel**

Replace `todo!()` in `SshManager::open_shell()`:
1. `client.channel_open_session()`
2. Request PTY: `channel.request_pty(false, "xterm-256color", cols, rows, 0, 0, &[])`
3. `channel.shell()`
4. Return channel for bidirectional I/O

- [ ] **Step 4: Implement auto-reconnect**

Add reconnect logic:
```rust
pub async fn reconnect(&self, server_id: &str) -> Result<(), String> {
    // Exponential backoff: 1s, 2s, 4s, 8s, 16s
    for attempt in 0..5u32 {
        let delay = std::time::Duration::from_secs(1 << attempt);
        tokio::time::sleep(delay).await;
        // Try to reconnect using stored config
        // If success, update status to Connected
        // If fail, update status to Reconnecting { attempt }
    }
    // All attempts failed
    Err("Failed to reconnect after 5 attempts".to_string())
}
```

- [ ] **Step 5: Verify it compiles**

Run: `cd src-tauri && cargo check`

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/ssh/mod.rs
git commit -m "feat: implement SSH connect, exec, and shell in SshManager"
```

---

### Task 18: Implement SshTmuxTransport

**Files:**
- Create: `src-tauri/src/transport/ssh_tmux.rs`
- Modify: `src-tauri/src/transport/mod.rs`

- [ ] **Step 1: Implement SshTmuxTransport**

```rust
// src-tauri/src/transport/ssh_tmux.rs
use super::{Transport, TransportError};
use crate::ssh::SshManager;
use async_trait::async_trait;
use std::sync::Arc;
use tauri::AppHandle;

pub struct SshTmuxTransport {
    session_id: i64,
    server_id: String,
    ssh_manager: Arc<SshManager>,
    channel: Arc<tokio::sync::Mutex<Option<russh::Channel<russh::client::Msg>>>>,
    alive: Arc<std::sync::atomic::AtomicBool>,
}

impl SshTmuxTransport {
    /// Create a new remote session:
    /// 1. tmux new-session on remote
    /// 2. tmux attach via interactive shell channel
    /// 3. Start background read task
    pub async fn spawn(
        session_id: i64,
        server_id: &str,
        agent_cmd: &str,
        cols: u16,
        rows: u16,
        ssh_manager: Arc<SshManager>,
        app: AppHandle,
        buffer_tx: tokio::sync::mpsc::UnboundedSender<(i64, Vec<u8>)>,
    ) -> Result<Self, TransportError> {
        let tmux_session_name = format!("racc-{}", session_id);

        // 1. Create tmux session with agent command
        ssh_manager.exec(server_id, &format!(
            "tmux new-session -d -s {} -x {} -y {} '{}'",
            tmux_session_name, cols, rows, agent_cmd
        )).await.map_err(|e| TransportError::IoError(e))?;

        // 2. Open interactive shell channel and attach to tmux
        let channel_id = ssh_manager.open_shell(server_id, cols, rows)
            .await.map_err(|e| TransportError::IoError(e))?;

        // 3. Send tmux attach command through the shell
        // channel.data(format!("tmux attach -t {}\n", tmux_session_name).as_bytes())

        // 4. Spawn background read task
        // Read from channel → emit transport:data event + buffer_tx

        let alive = Arc::new(std::sync::atomic::AtomicBool::new(true));

        Ok(Self {
            session_id,
            server_id: server_id.to_string(),
            ssh_manager,
            channel: Arc::new(tokio::sync::Mutex::new(None)), // Store actual channel
            alive,
        })
    }

    /// Reattach to an existing tmux session (after reconnect).
    pub async fn reattach(
        &self,
        app: AppHandle,
        buffer_tx: tokio::sync::mpsc::UnboundedSender<(i64, Vec<u8>)>,
    ) -> Result<(), TransportError> {
        let tmux_session_name = format!("racc-{}", self.session_id);
        // Open new shell channel, attach to existing tmux session
        // Restart background read task
        todo!()
    }
}

#[async_trait]
impl Transport for SshTmuxTransport {
    async fn write(&self, data: &[u8]) -> Result<(), TransportError> {
        let channel = self.channel.lock().await;
        if let Some(ref ch) = *channel {
            ch.data(data).await.map_err(|e| TransportError::IoError(e.to_string()))?;
        }
        Ok(())
    }

    async fn resize(&self, cols: u16, rows: u16) -> Result<(), TransportError> {
        let channel = self.channel.lock().await;
        if let Some(ref ch) = *channel {
            ch.window_change(cols as u32, rows as u32, 0, 0).await
                .map_err(|e| TransportError::IoError(e.to_string()))?;
        }
        Ok(())
    }

    async fn close(&self) -> Result<(), TransportError> {
        self.alive.store(false, std::sync::atomic::Ordering::SeqCst);
        let tmux_session_name = format!("racc-{}", self.session_id);
        let _ = self.ssh_manager.exec(
            &self.server_id,
            &format!("tmux kill-session -t {}", tmux_session_name),
        ).await;
        Ok(())
    }

    fn is_alive(&self) -> bool {
        self.alive.load(std::sync::atomic::Ordering::SeqCst)
    }
}
```

- [ ] **Step 2: Register module**

Add to `src-tauri/src/transport/mod.rs`:
```rust
pub mod ssh_tmux;
```

- [ ] **Step 3: Verify it compiles**

Run: `cd src-tauri && cargo check`

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/transport/ssh_tmux.rs src-tauri/src/transport/mod.rs
git commit -m "feat: implement SshTmuxTransport for remote tmux sessions"
```

---

### Task 19: Wire remote session creation

**Files:**
- Modify: `src-tauri/src/commands/session.rs`
- Modify: `src/stores/sessionStore.ts`

- [ ] **Step 1: Handle remote sessions in create_session**

Modify `create_session` in `session.rs` to handle `server_id.is_some()` case:

```rust
if let Some(ref sid) = server_id {
    // Remote session:
    // 1. Clone repo on remote if needed
    let remote_repo_path = format!("~/racc-repos/{}", repo_name);
    let check = ssh_manager.exec(sid, &format!("test -d {} && echo exists", remote_repo_path)).await;
    if !check.map_or(false, |o| o.stdout.contains("exists")) {
        ssh_manager.exec(sid, &format!("git clone {} {}", repo_url, remote_repo_path)).await
            .map_err(|e| format!("Failed to clone: {}", e))?;
    }

    // 2. Create worktree on remote
    let remote_worktree = format!("~/racc-worktrees/{}/{}", repo_name, branch);
    ssh_manager.exec(sid, &format!(
        "git -C {} worktree add {} -b {}",
        remote_repo_path, remote_worktree, branch
    )).await.map_err(|e| format!("Failed to create worktree: {}", e))?;

    // 3. Spawn SshTmuxTransport
    let agent_cmd = build_agent_command(&agent, &task_description, &remote_worktree);
    let transport = SshTmuxTransport::spawn(
        session_id, sid, &agent_cmd, 80, 24,
        ssh_manager_arc.clone(), app.clone(), transport_manager.buffer_sender(),
    ).await.map_err(|e| e.to_string())?;
    transport_manager.insert(session_id, Box::new(transport)).await;
} else {
    // Existing local session logic
}
```

- [ ] **Step 2: Update frontend session creation with server selection**

Modify `sessionStore.ts` `createSession` to pass `serverId`:
```typescript
createSession: async (repoId, taskDescription, agent, serverId?) => {
  const session = await invoke<Session>("create_session", {
    repoId, taskDescription, agent, serverId,
  });
  // ... rest unchanged, PTY is spawned by Rust
},
```

- [ ] **Step 3: Update task board / fire task UI to include server selector**

Add a server selector dropdown to the task firing UI. Values: "Local" + all connected servers from `useServerStore`.

- [ ] **Step 4: Verify it builds**

Run: `bun run build && cd src-tauri && cargo check`

- [ ] **Step 5: Manual test**

Test creating a remote session with a connected server.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/commands/session.rs src/stores/sessionStore.ts
git commit -m "feat: wire remote session creation through SshTmuxTransport"
```

---

### Task 20: Remote session reconciliation

**Files:**
- Modify: `src-tauri/src/commands/session.rs`

- [ ] **Step 1: Update reconcile_sessions for remote**

Modify `reconcile_sessions` (around line 458) to handle remote sessions:

```rust
// For each "Running" session:
if let Some(ref server_id) = session.server_id {
    // Remote session — check if tmux session still exists
    if ssh_manager.is_connected(server_id).await {
        let tmux_name = format!("racc-{}", session.id);
        match ssh_manager.exec(server_id, &format!("tmux has-session -t {}", tmux_name)).await {
            Ok(output) if output.exit_code == 0 => {
                // tmux session alive — keep Running, reattach transport
                let transport = SshTmuxTransport::spawn(/* reattach params */).await;
                transport_manager.insert(session.id, Box::new(transport)).await;
            }
            _ => {
                // tmux session gone — mark Completed
                update_session_status(&conn, session.id, "Completed")?;
            }
        }
    } else {
        // Can't reach server — mark Disconnected
        update_session_status(&conn, session.id, "Disconnected")?;
    }
} else {
    // Local session — existing logic: mark Disconnected
    update_session_status(&conn, session.id, "Disconnected")?;
}
```

- [ ] **Step 2: Verify it compiles**

Run: `cd src-tauri && cargo check`

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/commands/session.rs
git commit -m "feat: update reconcile_sessions to probe remote tmux sessions"
```

---

## Chunk 4: Polish (Phase 5)

### Task 21: Status bar connection indicators

**Files:**
- Modify: `src/components/StatusBar/StatusBar.tsx` (or wherever status bar lives)

- [ ] **Step 1: Add server connection status to status bar**

Display connected servers with status indicators:
```
GPU Box: ● connected | Dev VM: ○ reconnecting (2/5)
```

Use `useServerStore` to read server list and connection status. Subscribe to connection status change events from Rust via Tauri events.

- [ ] **Step 2: Commit**

```bash
git add src/components/StatusBar/
git commit -m "feat: show server connection status in status bar"
```

---

### Task 22: Auto-reconnect with UI feedback

**Files:**
- Modify: `src-tauri/src/ssh/mod.rs`
- Modify: `src-tauri/src/events.rs`

- [ ] **Step 1: Add ServerConnectionChanged event**

Add to `RaccEvent` enum in `events.rs`:
```rust
ServerConnectionChanged {
    server_id: String,
    status: String,  // "connected" | "disconnected" | "reconnecting"
    attempt: Option<u32>,
}
```

- [ ] **Step 2: Emit events during reconnection**

In `SshManager::reconnect()`, emit `ServerConnectionChanged` events at each attempt so the frontend can update the status bar.

- [ ] **Step 3: Trigger reconnect on transport failure**

In `SshTmuxTransport`'s background read task, when SSH channel drops:
1. Mark transport as not alive
2. Trigger `SshManager::reconnect()`
3. On success, call `SshTmuxTransport::reattach()`
4. On failure, emit Disconnected event

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/ssh/mod.rs src-tauri/src/events.rs
git commit -m "feat: auto-reconnect SSH with UI status events"
```

---

### Task 23: Final integration test

- [ ] **Step 1: Test local sessions still work**

Run: `bun tauri dev`
- Create a local session → verify terminal works
- Stop session → verify cleanup
- Restart app → verify reconciliation

- [ ] **Step 2: Test remote session flow**

- Add a server (manual config)
- Test connection
- Run setup wizard (both with and without API key)
- Create remote session → verify terminal output
- Kill SSH connection → verify auto-reconnect
- Restart Racc → verify remote session reconciliation

- [ ] **Step 3: Commit any fixes**

```bash
git commit -m "fix: integration test fixes for remote server support"
```
