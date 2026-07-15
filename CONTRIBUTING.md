# Contributing to Racc

Thanks for helping improve Racc. This guide describes the current desktop, headless, frontend, and core-library setup.

## Prerequisites

- [Rust](https://www.rust-lang.org/tools/install), stable toolchain
- [Bun](https://bun.sh/), v1.0 or newer
- Git
- The [Tauri 2.x system prerequisites](https://v2.tauri.app/start/prerequisites/) for your platform
- Claude Code and/or Codex CLI for exercising agent workflows

## Development Setup

```bash
git clone https://github.com/liu1700/racc.git
cd racc
bun install

# Full desktop application with hot reload
bun tauri dev
```

Useful commands:

```bash
bun run dev          # Vite frontend only, port 1420
bun run build        # TypeScript check and production frontend build
bun test             # Frontend/unit tests
bun tauri build      # Desktop production bundle

cd src-tauri
cargo check --workspace
cargo test -p racc-core
cargo build --bin racc-server

# Run browser build from the repository root's dist directory
RACC_DIST_PATH=../dist cargo run --bin racc-server
```

There is no single end-to-end test harness for the complete Tauri UI yet. For user-visible changes, combine the automated checks above with a focused desktop or headless-browser smoke test.

## Project Structure

```text
racc/
├── src/                         # React 19 + TypeScript shared frontend
│   ├── components/              # Task board, terminal, sidebar, servers, viewers
│   ├── services/                # RaccTransport and terminal integrations
│   ├── stores/                  # Zustand state
│   └── types/                   # Frontend domain types
├── src-tauri/
│   ├── racc-core/               # Shared business logic and transports
│   │   └── src/
│   │       ├── commands/        # Sessions, tasks, planner, merge, test, files, SSH
│   │       ├── transport/       # LocalPtyTransport and SshTmuxTransport
│   │       ├── db.rs            # SQLite schema and migrations
│   │       └── events.rs        # Cross-frontend event bus
│   ├── racc-server/             # Axum static server + /ws transport
│   └── src/                     # Thin Tauri command wrappers and native integrations
├── wiki/                        # Current guides plus clearly labelled design records
└── package.json
```

## Architecture Rules

- Put behavior shared by desktop and browser modes in `racc-core`, not in a Tauri wrapper.
- Frontend stores and components call `transport.call()` / `transport.on()`; do not introduce direct `invoke()` calls for shared behavior.
- Terminal processes are backend transports. Local sessions use native PTY; remote sessions use SSH/tmux.
- Planner, Merge Manager, and Test Manager use run-scoped loopback MCP endpoints for structured completion. Do not restore terminal-output JSON parsing as a state protocol.
- Keep manager MCP endpoints loopback-only and capability-scoped. Never persist or log bearer capabilities.
- Preserve user worktrees and unrelated local changes. Avoid destructive git cleanup in normal command paths.

## Making Changes

1. Create a focused branch from `main`.
2. Keep Tauri wrappers thin and add core tests for backend behavior where practical.
3. Update user-visible documentation when commands, UI labels, recovery semantics, or public WebSocket methods change.
4. Run checks proportional to the change.
5. Open a pull request with the behavior change, verification results, and any known limitations.

Recommended baseline before a pull request:

```bash
bun test
bun run build
cd src-tauri
cargo check --workspace
cargo test -p racc-core
```

## Commit Messages

Use conventional semantic prefixes:

- `feat:` new behavior
- `fix:` bug fix
- `docs:` documentation only
- `refactor:` behavior-preserving restructuring
- `test:` test-only changes
- `chore:` build, tooling, or dependency maintenance

## Code Style

Frontend code uses React 19, TypeScript, Zustand, and Tailwind. Reuse the custom `surface-*`, `accent`, and `status-*` tokens instead of adding isolated colors for established concepts.

Rust business logic belongs in `racc-core` and returns typed `CoreError` values. Tauri wrappers translate those results for IPC. Format only files you intentionally changed when the worktree already contains unrelated edits.
