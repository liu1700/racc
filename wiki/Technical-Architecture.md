# Technical Architecture

[< Home](Home.md) | [< UI Design](UI-Design.md)

## System Overview

Racc uses a **three-crate Rust workspace** (`src-tauri/`) that supports both a desktop Tauri app and a headless server:

| Crate | Type | Purpose |
|-------|------|---------|
| **racc-core** | Library | Shared business logic — `AppContext`, `EventBus`, `TransportManager`, commands, DB, SSH |
| **racc-server** | Binary | Headless HTTP/WebSocket server (Axum) — serves the React frontend as static files and exposes the same command set over JSON-RPC WebSocket |
| **racc** (root) | Tauri app | Desktop app — wraps `racc-core` with Tauri IPC and `tauri-plugin-pty` |

Both `racc` (Tauri) and `racc-server` construct the same `AppContext` struct from `racc-core`, ensuring identical behavior regardless of runtime.

### AppContext

`AppContext` (`racc-core/src/lib.rs`) is the shared application state:

```rust
pub struct AppContext {
    pub db: Arc<Mutex<Connection>>,         // SQLite
    pub transport_manager: TransportManager, // PTY + SSH session transports
    pub ssh_manager: Arc<SshManager>,        // SSH connection pool
    pub event_bus: Arc<dyn EventBus>,        // Abstracted event dispatch
    pub terminal_tx: broadcast::Sender<TerminalData>, // Terminal output broadcast
}
```

### EventBus Abstraction

`EventBus` (`racc-core/src/events.rs`) is a trait that decouples event emission from the runtime:

- **`BroadcastEventBus`** — default implementation using `tokio::sync::broadcast`, used by both Tauri and headless server.
- The Tauri app additionally bridges events to the WebView via `AppHandle.emit()`.
- The headless server fans events out to connected WebSocket clients.

### Frontend RaccTransport Layer

The React frontend uses a `RaccTransport` interface (`src/services/transport.ts`) to abstract over the communication channel:

- **`TauriTransport`** — wraps `invoke()` and `listen()` from `@tauri-apps/api`. Used when running inside the Tauri desktop app.
- **`WebSocketTransport`** — connects via `ws://<host>/ws` using JSON-RPC for commands and binary frames for terminal data. Used when served by `racc-server`.
- **Auto-detection** — `createTransport()` checks for `__TAURI_INTERNALS__` at startup and picks the appropriate transport. The rest of the frontend is transport-agnostic.

---

### Desktop Architecture (Tauri)

The desktop app uses the **single-process Tauri 2.x** architecture. The Rust backend and React frontend run in one process — the frontend calls Rust via `invoke()` IPC, and Rust handles all system interactions (PTY, git, SQLite, filesystem).

```
+----------------------------------------------------------------------+
|                        Tauri 2.x Application                         |
|                                                                      |
|  +---------------------------+     +-------------------------------+ |
|  |    React 19 Frontend      |     |     Rust Backend              | |
|  |  +---------------------+  | IPC |  +-------------------------+ | |
|  |  | Zustand Store       |  |<--->|  | Session Commands        | | |
|  |  | (sessionStore.ts)   |  |     |  | (session.rs)            | | |
|  |  +---------------------+  |     |  +-------------------------+ | |
|  |  | xterm.js Terminal   |  |     |  | Git Commands            | | |
|  |  | (Terminal.tsx)       |  |     |  | (git.rs)                | | |
|  |  +---------------------+  |     |  +-------------------------+ | |
|  |  | PTY Manager         |  |     |  | Cost Tracker            | | |
|  |  | (ptyManager.ts)     |  |     |  | (cost.rs)               | | |
|  |  +---------------------+  |     |  +-------------------------+ | |
|  |  | PTY Output Parser   |  |     |  | SQLite DB               | | |
|  |  | (ptyOutputParser.ts)|  |     |  | (db.rs)                 | | |
|  |  +---------------------+  |     |  +-------------------------+ | |
|  |  | UI Components       |  |     |                               | |
|  |  | Sidebar             |  |     |                               | |
|  |  +---------------------+  |     |  +-------------------------+ | |
|  +---------------------------+     +-------------------------------+ |
|                                                                      |
|  +------------------------------------------------------------------+|
|  |  tauri-plugin-pty: Native PTY processes (one per session)        ||
|  |  Agent runs inside PTY → xterm.js renders output in real-time   ||
|  +------------------------------------------------------------------+|
|                                                                      |
+----------------------------------------------------------------------+
```

## Layer Breakdown

| Layer | Component | Responsibility |
|-------|-----------|----------------|
| **Frontend** | React 19 + xterm.js + Zustand | Render UI, terminal display, state management |
| **IPC** | Tauri `invoke()` | Frontend ↔ Rust communication via `#[tauri::command]` |
| **Backend** | Rust (Tauri commands) | Session CRUD, git worktrees, token usage tracking |
| **Terminal I/O** | `tauri-plugin-pty` | Spawn/kill PTY processes, stream data to xterm.js |
| **Persistence** | SQLite | Repos and sessions stored in `~/.racc/racc.db` |
| **Insights Engine** | Frontend real-time rules + Rust batch analysis | Cross-session pattern detection — **hidden for MVP**, code preserved |
| **Communication** | Native PTY read/write | Agent-agnostic bidirectional terminal I/O |
| **Isolation** | Git Worktree (+ Docker planned) | Code isolation per session |
| **Agent Runtime** | Claude Code (Codex planned) | Pluggable — app does not bind to a specific agent |

## Tech Stack

### Client: Tauri 2.x

**Why Tauri over Electron:**
- Memory efficiency matters: users may have 5-10 terminal renderers + diff views open simultaneously
- Tauri's Rust backend handles all system interactions (PTY, git, SQLite) natively
- Single-process model simplifies deployment and state management

**Risk:** WebView cross-platform inconsistency (WebView2 on Windows, WKWebView on macOS, WebKitGTK on Linux). Requires extra cross-platform testing investment.

**macOS menu:** A custom minimal menu (Racc + Edit) replaces the default Tauri menu to prevent the macOS Help menu from intercepting keyboard events in the WebView terminal.

**Frontend stack:**
- React 19 + TypeScript 5.8
- xterm.js 5.5 with FitAddon for responsive terminal sizing
- Zustand 5 for state management
- Shiki for syntax highlighting (VS Code-compatible TextMate grammars, `github-dark-default` theme)
- Tailwind CSS 3.4 with custom design tokens
- Vite 6.3 for dev server and builds

### Session Persistence: SQLite + PTY

Repos and sessions are persisted in SQLite (`~/.racc/racc.db`). PTY processes provide runtime agent execution.

**Design:**
- Repos are first-class objects — imported via native folder picker (`tauri-plugin-dialog`), validated as git repos
- Each agent session = one native PTY process + one SQLite record
- Sessions can run directly in the repo or in an isolated git worktree
- On app startup, `reconcile_sessions()` marks all previously `Running` sessions as `Disconnected` (since PTY state is in-memory and lost on restart)
- On app close, `killAll()` cleans up all active PTY processes
- Token usage tracking reads Claude Code JSONL files from `~/.claude/projects/{encoded_path}/*.jsonl`

**Schema (v1):**
- `repos` table: id, path, name, added_at
- `sessions` table: id, repo_id, agent, worktree_path, branch, status, pr_url, server_id, created_at, updated_at
- `tasks` table: id, repo_id, description, images (JSON array), status, session_id, created_at, updated_at
- `session_events` table: id, session_id, event_type, payload (JSON), created_at (Unix ms)
- `insights` table: id, insight_type, severity, title, summary, detail_json, fingerprint (unique partial index on active), status, created_at, resolved_at
- `servers` table: id, name, host, port, username, auth_method, key_path, ssh_config_host, setup_status, setup_details, ai_provider, ai_api_key, created_at, updated_at

### Agent Communication: Native PTY

**Current implementation (Phase 2 — Direct PTY Bridging):**

```
Frontend (ptyManager.ts)  --[spawn]--> tauri-plugin-pty --> Shell + Agent
         xterm.js         <--[data]--- tauri-plugin-pty <-- Agent output
         xterm.js         --[input]--> tauri-plugin-pty --> Agent stdin
```

- `tauri-plugin-pty` spawns native PTY processes with configurable cols/rows
- Agent commands (e.g., `claude`) are sent to the shell after a brief startup delay
- Real-time bidirectional streaming: PTY output → xterm.js rendering, keyboard input → PTY stdin
- Terminal resize events are synced from xterm.js FitAddon to PTY
- Output buffer (up to 1MB per session) enables replay when switching between sessions
- `usePtyBridge.ts` React hook manages three effects: output subscription, input forwarding, resize sync

**Architectural note:** The original Phase 1 (tmux send-keys) was skipped in favor of direct PTY bridging, which provides real-time rendering and eliminates the need for output polling. Phase 3 (Agent SDK integration) remains planned for v0.3.

### Environment Isolation

| Strategy | When to Use | Status |
|----------|-------------|--------|
| **Bare Git Worktree** (default) | Lightweight projects, no env isolation needed | **Implemented** |
| **Docker Sandbox** (opt-in) | Need isolation, want `--dangerously-skip-permissions` | Planned v0.2 |

Worktrees are created at `~/racc-worktrees/{repo}/{branch}` via `git worktree add`.

**Not recommended for MVP:**
- Nix Flakes — learning curve too steep, narrows target audience
- Firecracker — overkill for individual developers

### WebSocket Remote API

Racc embeds a WebSocket server (`tokio-tungstenite`) on `ws://127.0.0.1:9399` that allows external clients to create tasks, start/stop sessions, and receive real-time status events. The server shares the same SQLite database and event bus as the UI — remote commands trigger PTY spawning in the frontend automatically.

See [WebSocket Remote API](WebSocket-Remote-API.md) for protocol details, available methods, and client examples.

| Module | File | Purpose |
|--------|------|---------|
| `events.rs` | `src-tauri/src/events.rs` | `RaccEvent` enum, `EventSender` broadcast channel |
| `ws_server.rs` | `src-tauri/src/ws_server.rs` | WebSocket server: TCP listener, connection pool, heartbeat, 10 method handlers, event fan-out |

### Networking: Tailscale + Portless *(planned v0.2)*

- Tailscale provides the mesh network between local and remote machines
- Portless assigns named URLs to worktree services
- **Cross-machine preview:** Use `Tailscale Serve` to expose Portless local addresses to the tailnet
- Result: `feature-auth.vps.tailnet` reaches the correct worktree's service from any machine

### Rust Command Modules

All Tauri commands are registered in `lib.rs` and organized into modules:

| Module | Commands | Purpose |
|--------|----------|---------|
| `session.rs` | `import_repo`, `list_repos`, `remove_repo`, `create_session`, `stop_session`, `remove_session`, `reattach_session`, `reconcile_sessions` | Session and repo lifecycle management |
| `git.rs` | `create_worktree`, `delete_worktree`, `get_diff` | Git worktree operations and diff |
| `cost.rs` | `get_project_costs` | Parse Claude Code JSONL usage files, aggregate token counts (total + weekly) |
| `task.rs` | `create_task`, `list_tasks`, `update_task_status`, `update_task_images`, `delete_task`, `save_task_image`, `copy_file_to_task_images`, `delete_task_image`, `rename_task_image` | Task CRUD for Task Board — create (with optional images), list by repo, update status/images, delete. Image file I/O: save from clipboard bytes, copy from file picker, delete, rename (draft→final) |
| `file.rs` | `read_file`, `search_files` | Read file content with language detection and truncation; fuzzy file search using `nucleo-matcher` with `.gitignore` support via `ignore` crate |
| `insights.rs` | `record_session_events`, `get_session_events`, `get_insights`, `save_insight`, `update_insight_status`, `run_batch_analysis`, `append_to_file` | Event recording, insight CRUD, batch analysis (repeated prompts via `strsim`, startup patterns, similar sessions), file append for CLAUDE.md |
| `db.rs` | `reset_db` | SQLite initialization, schema migrations (v1→v4), database reset (deletes and reinitializes `~/.racc/racc.db`) |
| `events.rs` | *(not a command module)* | `RaccEvent` enum, `EventSender` type alias, `create_event_bus()` factory |
| `ws_server.rs` | *(not a command module)* | WebSocket server on `127.0.0.1:9399` — 10 method handlers, event broadcast, heartbeat, graceful shutdown |

### Frontend Component Architecture

| Component | File | Purpose |
|-----------|------|---------|
| `App.tsx` | Root layout | Two-panel layout orchestrator (sidebar + center) with Tasks/Terminal/Servers tab switching, calls `initialize()` on mount |
| `Terminal.tsx` | Center panel | xterm.js renderer with FitAddon, async dynamic import |
| `Sidebar.tsx` | Left panel | Repo list with nested sessions, status indicators, quick actions |
| `NewAgentDialog.tsx` | Modal | Agent selector, skip-permissions toggle, worktree toggle, branch input |
| `RemoveSessionDialog.tsx` | Modal | Removal confirmation with optional worktree cleanup checkbox |
| `ResetDbDialog.tsx` | Modal | Database reset confirmation — wipes all repos, sessions, and assistant history |
| `ImportRepoDialog.tsx` | Modal | Native folder picker integration |
| `CostTracker.tsx` | *(not rendered)* | Polls `get_project_costs` every 10s — component exists but not in layout |
| `InsightsPanel.tsx` | *(not rendered)* | Insights timeline feed — code preserved for future use |
| `FileViewer.tsx` | Center panel (overlay) | Full file viewer with Shiki syntax highlighting, Cmd+F search, Ctrl+G jump-to-line |
| `CommandPalette.tsx` | Global overlay | Fuzzy file search (Cmd+P), keyboard navigation, debounced search |
| `fileViewerStore.ts` | Store | File viewer and command palette state — overlay, palette, search results, `openFile()` action |
| `insightsStore.ts` | Store | Insights state, real-time detection rules — **disabled for MVP** |
| `eventCapture.ts` | Service | Event normalization, buffering — **disabled for MVP** |
| `TaskBoard.tsx` | Center panel | 3-column kanban (Open/Working/Closed) with session sync |
| `TaskColumn.tsx` | Center panel | Single kanban column with header, cards, and new-task input |
| `TaskCard.tsx` | Center panel | Status-dependent card with live activity, fire button, and image thumbnails |
| `TaskInput.tsx` | Center panel | Inline task creation with image paste (Cmd+V), file picker, and thumbnail preview |
| `FireTaskDialog.tsx` | Modal | Task fire configuration — agent, worktree, auto-generated branch |
| `ServerPanel.tsx` | Center panel | Server management tab — add/connect/remove remote servers via SSH |
| `ServerList.tsx` | Center panel | Server list with expand/collapse actions (connect, disconnect, setup, edit, remove) |
| `taskStore.ts` | Store | Task CRUD, fireTask orchestration, session status sync |
| `DiffViewer.tsx` | *(not rendered)* | Placeholder — not currently planned |
| `StatusBar.tsx` | Bottom bar | Session counts, total/weekly token usage, connection status |

[Next: Session Lifecycle >](Session-Lifecycle.md)
