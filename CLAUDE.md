# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project

Racc is an Agentic IDE — a Tauri 2.x desktop app that orchestrates multiple AI coding agent sessions (Claude Code, Aider, Codex). It is not a code editor; it's a control plane for terminal-based agents built on native PTY, git worktrees, and xterm.js. Also runs as a headless server (`racc-server`) for browser-based access over Tailscale.

## Commands

```bash
bun install              # Install frontend dependencies
bun run dev              # Vite dev server (port 1420)
bun run build            # TypeScript check + Vite production build
bun tauri dev            # Launch full Tauri app (frontend + Rust backend)
bun tauri build          # Production desktop app build

# Rust only (from src-tauri/)
cargo check              # Type-check Rust backend
cargo build              # Build Rust backend
cargo check -p racc-core   # Type-check core library
cargo check -p racc-server # Type-check headless server
cargo build --bin racc-server  # Build headless server binary

# Headless server
RACC_DIST_PATH=../dist cargo run --bin racc-server  # Run on :9399
```

Environment variables for `racc-server`: `RACC_PORT` (default 9399), `RACC_DB_PATH` (default ~/.racc/racc.db), `RACC_DIST_PATH` (default "dist").

No test framework is configured yet.

## Architecture

**Three-crate Cargo workspace:**
- `src-tauri/racc-core/` — Core library with all business logic (commands, transport, SSH, DB, events). No Tauri dependency.
- `src-tauri/racc-server/` — Headless server binary (axum). Serves React UI + WebSocket API.
- `src-tauri/` — Tauri desktop app. Thin `#[tauri::command]` wrappers over `racc-core`.
- `src/` — React 19 + TypeScript frontend (shared between Tauri and browser modes).

**IPC pattern:** Frontend uses `RaccTransport` abstraction (`src/services/transport.ts`). Auto-detects Tauri (uses `invoke()`) vs browser (uses WebSocket). Stores and components call `transport.call()` / `transport.on()`.

**Core modules** (`src-tauri/racc-core/src/`):
- `commands/session.rs` — Create/list/stop sessions (DB + git worktree)
- `commands/task.rs` — Task CRUD + image management
- `commands/server.rs` — Remote server management (SSH)
- `transport/` — Transport trait, LocalPtyTransport, SshTmuxTransport, TransportManager
- `ssh/` — SSH client (russh), config parser
- `events.rs` — EventBus trait, BroadcastEventBus
- `db.rs` — SQLite schema, migrations

**Terminal I/O:** In Tauri mode, PTY data flows via `terminal_tx` broadcast → Tauri IPC event. In browser mode, PTY data flows via `terminal_tx` broadcast → WebSocket binary frames (8-byte session_id LE prefix).

**Frontend state:** Zustand stores call `transport.call(...)` for all backend operations.

**UI layout:** Two-panel — left sidebar (session list), center (tasks / xterm.js terminal), bottom status bar.

## Key Conventions

- **Session = PTY process + git worktree.** Each session spawns a native PTY via the transport layer (local or SSH/tmux).
- **Agent-agnostic:** Communication via native PTY read/write (works with any terminal agent)
- **Tailwind custom tokens:** `surface-{0,1,2,3}` for backgrounds, `accent` for interactive elements, `status-{running,waiting,paused,error,disconnected,completed}` for session states — defined in `tailwind.config.ts`
- **Path alias:** `@/*` maps to `src/*` in TypeScript (tsconfig only, not Vite — use relative imports)


## Wiki

Project design docs live in `wiki/` and are also published to the GitHub Wiki. See `wiki/Home.md` for navigation.
