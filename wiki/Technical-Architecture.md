# Technical Architecture

[< Home](Home.md) | [< UI Design](UI-Design.md)

## System Overview

Racc has one React frontend and one shared Rust business-logic core, exposed through two primary runtime surfaces.

```text
                     React 19 frontend
                  (components + Zustand)
                           |
                      RaccTransport
                    /               \
             Tauri IPC          WebSocket /ws
                |                    |
        thin Tauri wrappers      racc-server (Axum)
                    \               /
                         racc-core
          commands | SQLite | events | transports | MCP
                         /       \
                  local PTY     SSH/tmux
```

## Rust Workspace

The Cargo workspace under `src-tauri/` contains:

| Crate | Purpose |
|-------|---------|
| `racc-core` | Runtime-independent commands, SQLite migrations, event bus, git/file access, local/remote transports, Task Planner, Merge Manager, Test Manager, and MCP endpoints |
| `racc-server` | Axum binary serving the production frontend and `/ws` for browser mode |
| `racc` | Tauri desktop application with native menus/plugins and thin command wrappers over `racc-core` |

Shared behavior belongs in `racc-core`. Tauri-only code is limited to IPC annotations, desktop plugins, native menu behavior, the assistant sidecar, and event forwarding.

## Shared Application State

Both primary runtimes construct `AppContext`:

```rust
pub struct AppContext {
    pub db: Arc<Mutex<Connection>>,
    pub transport_manager: TransportManager,
    pub ssh_manager: Arc<SshManager>,
    pub event_bus: Arc<dyn EventBus>,
    pub terminal_tx: broadcast::Sender<TerminalData>,
}
```

- `db` is the SQLite connection shared by command handlers.
- `TransportManager` owns live transports and bounded per-session buffers.
- `SshManager` manages remote connections.
- `EventBus` carries domain state changes.
- `terminal_tx` carries raw terminal bytes separately from low-volume domain events.

## Frontend Transport Boundary

`src/services/transport.ts` exposes:

```typescript
interface RaccTransport {
  call(method: string, params?: Record<string, unknown>): Promise<unknown>;
  on(event: string, handler: (data: unknown) => void): () => void;
  onTerminalData(sessionId: number, handler: (data: Uint8Array) => void): () => void;
  isLocal(): boolean;
}
```

`createTransport()` checks for the Tauri runtime:

- **TauriTransport** dynamically loads `invoke` and `listen`. Core events arrive through `racc://event`; terminal bytes arrive through `transport:data`.
- **WebSocketTransport** connects to the current host's `/ws`. It converts top-level camelCase frontend arguments to snake_case, correlates JSON request IDs, dispatches events, and demultiplexes binary terminal frames.

Stores and shared components must use this boundary so browser and desktop behavior remain aligned.

## Session Transports

### LocalPtyTransport

Local sessions use `portable-pty`, not a frontend PTY plugin. `racc-core` starts the user's shell, launches Claude Code or Codex, reads/writes bytes, resizes the terminal, and reports exit/liveness state.

### SshTmuxTransport

Remote sessions connect through `russh` and execute inside named tmux sessions. Tmux provides persistence beyond a browser or desktop restart. Reconciliation and reconnect probe the remote session before deciding whether it is Running, Completed, or Disconnected.

### Buffering and Delivery

`TransportManager` maintains a bounded ring buffer per session. Transport readers publish `TerminalData { session_id, data }`:

- Tauri forwards it as `transport:data` to the WebView.
- `racc-server` sends `i64 session_id` in 8-byte little-endian form followed by raw bytes in a WebSocket binary frame.

Browser terminal input uses the same binary layout in the opposite direction. Resize, liveness, and buffer retrieval use command calls.

## Commands and Domain Modules

Current `racc-core/src/commands/` modules are grouped by responsibility:

| Module | Responsibility |
|--------|----------------|
| `session.rs` | Repository import, normal/specialized session creation, stop/remove, reconnect/reattach, reconciliation, PR metadata |
| `task.rs` | Task CRUD and task image file operations |
| `planner.rs`, `planner_mcp.rs` | Read-only task planning runs, preview validation, selective task creation, planner MCP submission |
| `merge.rs` | Per-repository merge settings, ordered queue, integration runs, resolution/retry |
| `test_manager.rs` | Per-repository test settings, isolated UAT runs, resolution/retry |
| `manager_mcp.rs` | Capability-scoped Merge/Test MCP runtime and validated result persistence |
| `transport.rs` | Terminal write, resize, liveness, and buffer commands |
| `server.rs`, `setup.rs` | SSH server CRUD, connection management, remote setup and commands |
| `git.rs` | Worktree create/delete and diff operations |
| `file.rs` | Safe repository file reading and fuzzy search |
| `cost.rs`, `insights.rs` | Claude usage aggregation and preserved insight/event facilities |

The Tauri crate mirrors public modules with small `#[tauri::command]` wrappers. `racc-server/src/ws.rs` dispatches WebSocket method names directly to the same core functions.

## Structured Workflow MCP

Task Planner, Merge Manager, and Test Manager need typed results that can update the board even when terminal output contains arbitrary prose.

At run start, `racc-core` creates a loopback listener on an ephemeral port and a random bearer capability. The endpoint and capability are injected only into that run's agent environment. The agent connects using MCP and must call one tool:

| Workflow | Server/tool |
|----------|-------------|
| Task Planner | `racc_task_plan.submit_task_plan` |
| Merge Manager | `racc_merge_manager.submit_merge_result` |
| Test Manager | `racc_test_manager.submit_test_result` |

The handler verifies authorization, tool name, run ID, repository ID, status, and payload structure before writing SQLite. It then emits `task_plan_changed`, `merge_manager_changed`, or `test_manager_changed`.

These endpoints are intentionally short-lived and run-scoped. They are not general Racc administration APIs. Terminal sentinel strings such as `RACC_SHIP_RESULT:{...}` or printed result JSON are not parsed.

## Event Model

`RaccEvent` is serialized as `{ "event": "...", "data": {...} }` and currently includes:

- `session_status_changed`
- `task_status_changed`
- `task_deleted`
- `task_plan_changed`
- `merge_manager_changed`
- `test_manager_changed`

The broadcast bus feeds both Tauri's WebView bridge and WebSocket clients. Events tell clients to update or refetch state; SQLite remains the durable source of truth.

## Persistence

The default database is `~/.racc/racc.db`. Schema version 6 includes:

- repositories and sessions, including agent conversation ID and permission choice;
- tasks, task images metadata, supervisor fields, session events, and insights;
- SSH server configurations;
- task-plan runs;
- merge settings, merge runs, and merge queue items;
- test settings and test runs.

Task attachment files live at `{repo}/.racc/images/`. Worktrees normally live under `~/racc-worktrees/` and manager branches use `racc/ship-*` or `racc/test-*` naming.

## Desktop Runtime

The Tauri app builds an `AppContext`, starts its buffer task, forwards domain/terminal events to the WebView, registers native shell/dialog/notification plugins, and exposes command wrappers.

It also retains a localhost-only compatibility WebSocket server on `127.0.0.1:9399` for external automation clients. That endpoint is a smaller text-only API and is not the transport used by the desktop React UI. See [WebSocket Remote API](WebSocket-Remote-API.md).

## Headless Runtime

`racc-server`:

1. opens the configured SQLite database;
2. creates transports, SSH manager, event bus, and terminal channel;
3. reconciles stale sessions;
4. serves static frontend assets;
5. upgrades `/ws` connections for commands, events, and terminal bytes.

Configuration:

| Variable | Default |
|----------|---------|
| `RACC_PORT` | `9399` |
| `RACC_DB_PATH` | `~/.racc/racc.db` |
| `RACC_DIST_PATH` | `dist` |

The listener binds to `0.0.0.0`. There is currently no application authentication or TLS, so the deployment boundary must be a trusted private network.

## Isolation and Security Boundaries

- Git worktrees isolate branches and working-tree changes; they do not restrict filesystem or network access.
- Permission-bypass options are passed to the selected agent and should be used only in an environment the user trusts.
- Manager MCP binds to loopback and uses a per-run secret capability.
- Headless access should not be exposed directly to the public internet.
- External URL opening accepts only HTTP(S).

[Next: Session Lifecycle >](Session-Lifecycle.md)
