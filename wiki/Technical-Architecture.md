# Technical Architecture

[< Home](Home.md) | [< UI Design](UI-Design.md)

## System Overview

Racc uses a **single-process Tauri 2.x** architecture. The Rust backend and React frontend run in one process — the frontend calls Rust via `invoke()` IPC, and Rust handles all system interactions (PTY, git, SQLite, filesystem).

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
|  |  | Sidebar/ActivityPanel|  |     |                               | |
|  |  +---------------------+  |     |  +-------------------------+ | |
|  +---------------------------+     +-------------------------------+ |
|                                                                      |
|  +------------------------------------------------------------------+|
|  |  tauri-plugin-pty: Native PTY processes (one per session)        ||
|  |  Agent runs inside PTY → xterm.js renders output in real-time   ||
|  +------------------------------------------------------------------+|
|  +------------------------------------------------------------------+|
|  |  Sidecar: racc-assistant (bun-compiled binary, stdin/stdout JSON) ||
|  |  pi-ai + pi-agent-core → OpenRouter → LLM                       ||
|  +------------------------------------------------------------------+|
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
| **Insights Engine** | Frontend real-time rules + Rust batch analysis | Cross-session pattern detection (file conflicts, repeated prompts, cost anomalies, similar sessions) |
| **Communication** | Native PTY read/write | Agent-agnostic bidirectional terminal I/O |
| **Isolation** | Git Worktree (+ Docker planned) | Code isolation per session |
| **Agent Runtime** | Claude Code / Aider / Codex | Pluggable — IDE does not bind to a specific agent |

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

**Schema (v4):**
- `repos` table: id, path, name, added_at
- `sessions` table: id, repo_id, agent, worktree_path, branch, status, created_at, updated_at
- `assistant_messages` table: id, role, content, tool_name, tool_call_id, created_at
- `assistant_config` table: key, value
- `session_events` table: id, session_id (FK→sessions), event_type, payload (JSON), created_at (Unix ms)
- `insights` table: id, insight_type, severity, title, summary, detail_json, fingerprint (unique partial index on active), status, created_at, resolved_at
- Migration v1→v2 dropped deprecated `tmux_session_name` column
- Migration v2→v3 added assistant tables
- Migration v3→v4 added session_events and insights tables for the Insights Panel

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
| `assistant.rs` | `set_assistant_config`, `get_assistant_config`, `save_assistant_message`, `get_assistant_messages`, `get_all_sessions_for_assistant`, `get_session_diff_for_assistant`, `get_session_costs_for_assistant`, `read_file_for_assistant`, `assistant_send_message`, `assistant_read_response`, `assistant_shutdown` | AI assistant config, message persistence, session queries, file reading relay, sidecar process management |
| `file.rs` | `read_file`, `search_files` | Read file content with language detection and truncation; fuzzy file search using `nucleo-matcher` with `.gitignore` support via `ignore` crate |
| `insights.rs` | `record_session_events`, `get_session_events`, `get_insights`, `save_insight`, `update_insight_status`, `run_batch_analysis`, `append_to_file` | Event recording, insight CRUD, batch analysis (repeated prompts via `strsim`, startup patterns, similar sessions), file append for CLAUDE.md |
| `db.rs` | (internal) | SQLite initialization, schema migrations (v1–v4) |

### Frontend Component Architecture

| Component | File | Purpose |
|-----------|------|---------|
| `App.tsx` | Root layout | Three-panel layout orchestrator, calls `initialize()` on mount |
| `Terminal.tsx` | Center panel | xterm.js renderer with FitAddon, async dynamic import |
| `Sidebar.tsx` | Left panel | Repo list with nested sessions, status indicators, quick actions |
| `NewAgentDialog.tsx` | Modal | Agent selector, skip-permissions toggle, worktree toggle, branch input |
| `RemoveSessionDialog.tsx` | Modal | Removal confirmation with optional worktree cleanup checkbox |
| `ImportRepoDialog.tsx` | Modal | Native folder picker integration |
| `CostTracker.tsx` | Right panel | Polls `get_project_costs` every 10s, displays token usage breakdown |
| `InsightsPanel.tsx` | Right panel | Insights timeline feed — replaces previous AssistantPanel |
| `InsightCard.tsx` | Right panel | Single insight card with collapsed/expanded states and evidence display |
| `InsightActions.tsx` | Right panel | Per-type action buttons (Add to CLAUDE.md, View File, Switch session, etc.) |
| `AssistantSetup.tsx` | Right panel | API key configuration — accessed via settings gear in InsightsPanel |
| `FileViewer.tsx` | Center panel (overlay) | Full file viewer with Shiki syntax highlighting, Cmd+F search, Ctrl+G jump-to-line |
| `CommandPalette.tsx` | Global overlay | Fuzzy file search (Cmd+P), keyboard navigation, debounced search |
| `fileViewerStore.ts` | Store | File viewer and command palette state — overlay, palette, search results, `openFile()` action |
| `insightsStore.ts` | Store | Insights state, real-time detection rules (file conflicts, cost anomalies, permissions), Tauri event listener |
| `eventCapture.ts` | Service | Event normalization, buffering, 30s batch flush to SQLite, real-time listener dispatch |
| `DiffViewer.tsx` | Center panel | Placeholder (P1 feature) |
| `StatusBar.tsx` | Bottom bar | Session counts, total/weekly token usage, connection status |

[Next: Session Lifecycle >](Session-Lifecycle.md)
