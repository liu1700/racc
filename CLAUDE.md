# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project

Racc is a Tauri 2.x desktop app that orchestrates AI coding agent sessions. Currently supports Claude Code, with Codex support planned. It is not a code editor; it's a control plane for terminal-based agents built on native PTY, git worktrees, and xterm.js.

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
```

No test framework is configured yet.

## Architecture

**Tauri 2.x Client/Server in one process:**
- `src/` — React 19 + TypeScript frontend rendered in system WebView
- `src-tauri/` — Rust backend handling all system interactions

**IPC pattern:** Frontend calls Rust via `invoke()` from `@tauri-apps/api/core`. Rust commands use `#[tauri::command]` macro and return `Result<T, String>`. All commands are registered in `src-tauri/src/lib.rs`.

**Rust command modules** (`src-tauri/src/commands/`):
- `session.rs` — Create/list/stop sessions (DB + git worktree)
- `git.rs` — Worktree create/delete, diff
- `cost.rs` — Read Claude Code usage data from `~/.claude/usage/`

**Terminal I/O:** Managed entirely from the frontend via `tauri-plugin-pty`. The `ptyManager.ts` singleton spawns/kills PTY processes; `usePtyBridge.ts` hook streams output to xterm.js in real-time.

**Frontend state:** Zustand store in `src/stores/sessionStore.ts` manages session list and active session, calls Tauri commands.

**UI layout:** Two-panel — left sidebar (session list), center (tasks / xterm.js terminal), bottom status bar.

## Key Conventions

- **Session = PTY process + git worktree.** Sessions are created through tasks (fireTask), not directly. Each session spawns a native PTY via `tauri-plugin-pty`.
- **Agent-agnostic:** Communication via native PTY read/write (works with any terminal agent)
- **Tailwind custom tokens:** `surface-{0,1,2,3}` for backgrounds, `accent` for interactive elements, `status-{running,waiting,paused,error,disconnected,completed}` for session states — defined in `tailwind.config.ts`
- **Path alias:** `@/*` maps to `src/*` in TypeScript (tsconfig only, not Vite — use relative imports)

## Wiki

Project design docs live in `wiki/` and are also published to the GitHub Wiki. See `wiki/Home.md` for navigation.
