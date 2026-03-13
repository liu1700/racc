# Racc

An Agentic IDE — a desktop app that orchestrates multiple AI coding agent sessions (Claude Code, Aider, Codex) in parallel. Not a code editor; a control plane for terminal-based agents built on native PTY, git worktrees, and xterm.js.

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

## Setup

```bash
# Clone
git clone https://github.com/liu1700/racc.git
cd racc

# Install frontend dependencies
bun install

# Build the AI assistant sidecar binary
cd sidecar
bun install
bash build.sh
cd ..
```

## Development

```bash
bun tauri dev          # Launch full app (frontend + Rust backend)
```

This starts the Vite dev server on port 1420 and compiles/launches the Rust backend with hot reload.

### Other commands

```bash
bun run dev            # Vite dev server only (no Rust backend)
bun run build          # TypeScript check + Vite production build

# Rust only (from src-tauri/)
cargo check            # Type-check Rust backend
cargo build            # Build Rust backend

# Sidecar only (from sidecar/)
bash build.sh          # Rebuild assistant sidecar binary
```

## Production Build

```bash
bun tauri build
```

Outputs a platform-specific installer in `src-tauri/target/release/bundle/`.

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
│   │   ├── assistant.rs    # AI assistant + sidecar management
│   │   └── db.rs           # SQLite initialization and migrations
│   └── binaries/           # Sidecar binaries (git-ignored)
├── sidecar/                # AI assistant sidecar (TypeScript, bun-compiled)
│   └── src/
│       ├── index.ts        # stdin/stdout JSON lines protocol
│       ├── agent.ts        # LLM agent setup and system prompt
│       ├── tools.ts        # Tool definitions (sessions, diffs, costs)
│       └── protocol.ts     # Message type definitions
├── wiki/                   # Design docs (synced to GitHub Wiki)
└── package.json
```

## Architecture

Three-panel layout: left sidebar (session list), center (xterm.js terminal), right panel (cost tracker + AI assistant).

Each agent session = one native PTY process + one git worktree. The app is agent-agnostic — it works with any terminal-based coding agent.

The AI assistant runs as a sidecar binary, communicating with the Rust backend over stdin/stdout JSON lines. It uses OpenRouter for LLM access and can summarize diffs, triage review risk, and answer questions about any session.

See the [wiki](https://github.com/liu1700/racc/wiki) for detailed design docs.

## License

TBD
