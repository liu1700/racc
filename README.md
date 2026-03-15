<p align="center">
  <img src="assets/logo.png" alt="Racc" width="200" />
</p>

<h1 align="center">Racc</h1>

<p align="center">
  A desktop control plane for orchestrating AI coding agents.
</p>

<p align="center">
  <a href="https://github.com/liu1700/racc/blob/main/LICENSE"><img src="https://img.shields.io/badge/license-MIT-blue.svg" alt="License" /></a>
  <a href="https://github.com/liu1700/racc/releases"><img src="https://img.shields.io/github/v/release/liu1700/racc?include_prereleases" alt="Release" /></a>
</p>

---

<!-- TODO: Add screenshot or GIF here -->
<!-- <p align="center">
  <img src="assets/screenshot.png" alt="Racc Screenshot" width="800" />
</p> -->

## What is Racc?

Racc is a desktop app that lets you run multiple AI coding agents in parallel — each in its own terminal, its own git worktree, fully isolated. It's not a code editor. It's the control plane you've been missing.

Currently supports **Claude Code**, with **Codex** support planned.

## Features

- **Multi-agent sessions** — Run multiple agent sessions side by side
- **Agent-agnostic** — Communicates via native PTY, works with any agent that runs in a terminal
- **Git worktree isolation** — Each session gets its own worktree automatically, no conflicts
- **Task board** — Kanban-style board for cognitive offloading and automated agent orchestration

## Roadmap

| Milestone | Description | Status |
|-----------|-------------|--------|
| **v0.1 — MVP** | Multi-session dashboard, task board, git worktree isolation, file viewer | Done |
| **v0.2 — Multi-Agent & Isolation** | Codex support, Docker sandbox, remote sessions | Planned |
| **v0.3** | TBD | — |

## Quick Start

**Prerequisites:** [Rust](https://www.rust-lang.org/tools/install) (stable) | [Bun](https://bun.sh/) (v1.0+) | [Git](https://git-scm.com/) | [Tauri 2.x prerequisites](https://v2.tauri.app/start/prerequisites/)

```bash
git clone https://github.com/liu1700/racc.git
cd racc
bun install

# Launch
bun tauri dev
```

## Architecture

Two-panel layout: session list (left sidebar), tasks / terminal (center). Each session = one PTY process + one git worktree. Built with Tauri 2.x (Rust backend + React 19 frontend).

See the [wiki](https://github.com/liu1700/racc/wiki) for detailed design docs, including [Technical Architecture](https://github.com/liu1700/racc/wiki/Technical-Architecture) and [Cognitive Design Research](https://github.com/liu1700/racc/wiki/Cognitive-Design-Research).

## Why "Racc"?

Short for **raccoon** — clever, adorable, with nimble little hands. But be careful — they can be surprisingly brutal sometimes.

## Contributing

We welcome contributions! See [CONTRIBUTING.md](CONTRIBUTING.md) for setup instructions and guidelines.

## License

[MIT](LICENSE)
