# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project

OTTE is an Agentic IDE — a Tauri 2.x desktop app that orchestrates multiple AI coding agent sessions (Claude Code, Aider, Codex). It is not a code editor; it's a control plane for terminal-based agents built on tmux, git worktrees, and xterm.js.

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
- `session.rs` — Create/list/stop sessions (wraps tmux + git worktree)
- `tmux.rs` — send-keys and capture-pane
- `git.rs` — Worktree create/delete, diff
- `cost.rs` — Read Claude Code usage data from `~/.claude/usage/`

**Frontend state:** Zustand store in `src/stores/sessionStore.ts` manages session list and active session, calls Tauri commands.

**UI layout:** Three-panel — left sidebar (session list, 15%), center (xterm.js terminal, 55%), right panel (cost tracker + activity log, 30%), bottom status bar.

## Key Conventions

- **Session = tmux session + git worktree.** Naming: `otte-{project}-{branch}`
- **Agent-agnostic:** Communication via tmux send-keys/capture-pane (works with any terminal agent)
- **Tailwind custom tokens:** `surface-{0,1,2,3}` for backgrounds, `accent` for interactive elements, `status-{running,waiting,paused,error,disconnected,completed}` for session states — defined in `tailwind.config.ts`
- **Path alias:** `@/*` maps to `src/*` in TypeScript

## Wiki

Project design docs live in `wiki/` and are also published to the GitHub Wiki. See `wiki/Home.md` for navigation.
