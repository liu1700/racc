<p align="center">
  <img src="assets/logo.png" alt="Racc" width="200" />
</p>

<h1 align="center">Racc</h1>

<p align="center">
  A desktop and browser control plane for orchestrating AI coding agents.
</p>

<p align="center">
  <a href="https://github.com/liu1700/racc/blob/main/LICENSE"><img src="https://img.shields.io/badge/license-MIT-blue.svg" alt="License" /></a>
  <a href="https://github.com/liu1700/racc/releases"><img src="https://img.shields.io/github/v/release/liu1700/racc?include_prereleases" alt="Release" /></a>
</p>

---

<p align="center">
  <img src="assets/demo.gif" alt="Racc Demo" width="800" />
</p>

## What is Racc?

Racc lets you plan, launch, monitor, test, and merge work produced by terminal-based AI coding agents. Every normal task can run in its own git worktree and native terminal session, so multiple agents can work in parallel without sharing a working tree.

Racc is a control plane, not a code editor. It currently supports **Claude Code** and **Codex** in both local and remote workflows.

## Features

- **Task Board** — Four focused columns: Open, Working, Merge Manager, and Test Manager. Completed tasks are archived instead of occupying a visible Closed column.
- **Task Planner** — Give Claude Code or Codex an Epic link or product description, review the generated dependency-aware plan, and create only the tasks you select.
- **Isolated agent sessions** — Fire a task into the repository or a dedicated git worktree, with an interactive xterm.js terminal and persistent session metadata.
- **Merge Manager** — Queue PRs from Working, configure the target branch and instructions per repository, test the combined tree, and merge the queue in order.
- **Test Manager** — Run an isolated, read-only full-project UAT pass. The default prompt is editable per repository and the action button is simply **Run**.
- **Structured manager results** — Planner, Merge Manager, and Test Manager report through capability-scoped loopback MCP tools. Racc stores the result directly and refreshes the UI; printed JSON is not used as a completion protocol.
- **Direct terminal links** — HTTP(S) links open in the system browser without xterm's generic warning dialog. Detected file paths open in Racc's file viewer, including optional line numbers.
- **Remote servers** — Run agents over SSH in persistent tmux sessions and reconnect from the dashboard.
- **Headless server** — `racc-server` serves the same React UI and WebSocket transport for browser access, including terminal streaming.
- **Usage visibility** — Aggregate Claude Code token usage and automatically configure RTK output compression when available.

See the [User Guide](wiki/User-Guide.md) for the end-to-end workflow and recovery behavior.

## Download

Grab the latest macOS `.dmg` from the [Releases](https://github.com/liu1700/racc/releases) page. Linux and Windows users can currently build from source with the Tauri prerequisites for their platform.

## Build from Source

**Prerequisites:** [Rust](https://www.rust-lang.org/tools/install) (stable), [Bun](https://bun.sh/) (v1.0+), [Git](https://git-scm.com/), and the [Tauri 2.x prerequisites](https://v2.tauri.app/start/prerequisites/) for your platform.

```bash
git clone https://github.com/liu1700/racc.git
cd racc
bun install

# Desktop app
bun tauri dev

# Headless server (browser access)
bun run build
cd src-tauri
RACC_DIST_PATH=../dist cargo run --bin racc-server
# Open http://localhost:9399
```

`racc-server` supports `RACC_PORT` (default `9399`), `RACC_DB_PATH` (default `~/.racc/racc.db`), and `RACC_DIST_PATH` (default `dist`). It binds to `0.0.0.0` and currently has no application-level authentication or TLS. Expose it only on a trusted network such as a private Tailscale tailnet.

## Development Checks

```bash
bun test
bun run build

cd src-tauri
cargo check --workspace
cargo test -p racc-core
```

## Architecture

Racc uses a three-crate Rust workspace:

- `racc-core` owns business logic, SQLite, local PTY and SSH/tmux transports, workflow managers, and events.
- `racc-server` is an Axum HTTP/WebSocket server for the browser build.
- The Tauri desktop crate provides thin command wrappers and native desktop integrations.

The shared React frontend talks through `RaccTransport`, which selects Tauri IPC in the desktop app or WebSocket in a browser. See [Technical Architecture](wiki/Technical-Architecture.md), [Session Lifecycle](wiki/Session-Lifecycle.md), and [WebSocket Remote API](wiki/WebSocket-Remote-API.md).

## Why "Racc"?

Short for **raccoon** — clever, adorable, with nimble little hands. But be careful: they can be surprisingly brutal sometimes.

<p align="center">
  <img src="assets/raccoon.png" alt="Raccoon" width="300" />
</p>

## Contributing

Contributions are welcome. See [CONTRIBUTING.md](CONTRIBUTING.md) for setup, architecture, and verification guidance.

## License

[MIT](LICENSE)
