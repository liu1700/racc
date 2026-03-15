# Contributing to Racc

Thanks for your interest in contributing! This guide covers everything you need to get started.

## Prerequisites

- [Rust](https://www.rust-lang.org/tools/install) (stable toolchain)
- [Bun](https://bun.sh/) (v1.0+)
- [Git](https://git-scm.com/)
- System dependencies for Tauri 2.x (see [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/))

### Linux (Debian/Ubuntu)

```bash
# Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source ~/.cargo/env

# Bun
curl -fsSL https://bun.sh/install | bash

# Tauri system dependencies
sudo apt update
sudo apt install libwebkit2gtk-4.1-dev build-essential curl wget file \
  libxdo-dev libssl-dev libayatana-appindicator3-dev librsvg2-dev
```

### macOS

```bash
# Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source ~/.cargo/env

# Bun
curl -fsSL https://bun.sh/install | bash

# Xcode Command Line Tools (required by Tauri)
xcode-select --install
```

### Windows

```powershell
# Rust — download installer from https://www.rust-lang.org/tools/install
# Bun
powershell -c "irm bun.sh/install.ps1 | iex"
# WebView2 and Visual Studio C++ Build Tools required — see Tauri docs
```

## Development Setup

```bash
git clone https://github.com/liu1700/racc.git
cd racc
bun install

# Launch full app (frontend + Rust backend with hot reload)
bun tauri dev
```

### Other Commands

```bash
bun run dev            # Vite dev server only (no Rust backend)
bun run build          # TypeScript check + Vite production build

# Rust only (from src-tauri/)
cargo check            # Type-check Rust backend
cargo build            # Build Rust backend

# Production build
bun tauri build        # Outputs installer in src-tauri/target/release/bundle/
```

## Project Structure

```
racc/
├── src/                    # React 19 + TypeScript frontend
│   ├── components/         # UI components (Sidebar, Terminal, Assistant, etc.)
│   ├── stores/             # Zustand state management
│   └── types/              # TypeScript type definitions
├── src-tauri/              # Rust backend (Tauri 2.x)
│   ├── src/commands/       # Tauri command modules
│   │   ├── session.rs      # Session and repo lifecycle
│   │   ├── git.rs          # Git worktree operations
│   │   ├── cost.rs         # Claude Code cost tracking
│   │   └── db.rs           # SQLite initialization and migrations
├── wiki/                   # Design docs (synced to GitHub Wiki)
└── package.json
```

## Making Changes

1. Fork the repo and create a branch from `main`
2. Make your changes
3. Ensure `bun run build` passes (TypeScript check + Vite build)
4. Ensure `cargo check` passes in `src-tauri/`
5. Open a pull request

### Commit Messages

Use semantic prefixes:

- `feat:` — new feature
- `fix:` — bug fix
- `docs:` — documentation changes
- `refactor:` — code restructuring without behavior change
- `chore:` — build, tooling, or dependency updates

## Code Style

**Frontend (src/):**
- TypeScript + React 19
- Zustand for state management
- Tailwind CSS with custom tokens: `surface-{0,1,2,3}` for backgrounds, `accent` for interactive elements, `status-{running,waiting,paused,error,disconnected,completed}` for session states

**Backend (src-tauri/):**
- Rust with Tauri 2.x
- Commands use `#[tauri::command]` macro, return `Result<T, String>`
- All commands registered in `src-tauri/src/lib.rs`
