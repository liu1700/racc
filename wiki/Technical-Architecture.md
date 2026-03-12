# Technical Architecture

[< Home](Home) | [< UI Design](UI-Design)

## System Overview

OTTE uses a **Remote-First Client/Server** architecture. The client (Tauri) is a lightweight, stateless renderer. A daemon process on each machine manages all state.

```
+---------------------------+          +---------------------------+
|     Tauri Client          |          |     Daemon (per machine)  |
|  +---------------------+ |  WebSocket  +---------------------+ |
|  | React + xterm.js    |<|----------|>| Session Manager       | |
|  | (WebView)           | |          |  | Cost Tracker          | |
|  +---------------------+ |          |  | Git/Worktree Manager  | |
|  | Rust Core            | |          |  | TMux Controller       | |
|  | (IPC bridge)         | |          |  | Docker Manager        | |
|  +---------------------+ |          |  +---------------------+ |
+---------------------------+          |  | PTY / send-keys      | |
                                       |  +----------+-----------+ |
           Tailscale Mesh              |             |             |
        (cross-machine)               |  +----------v-----------+ |
                                       |  | Agent Runtime         | |
                                       |  | (Claude Code / Aider) | |
                                       |  +---------------------+ |
                                       +---------------------------+
```

## Layer Breakdown

| Layer | Component | Responsibility |
|-------|-----------|----------------|
| **Client** | Tauri (WebView + Rust) | Render UI, forward user actions, xterm.js terminal |
| **Network** | Tailscale Mesh | Connect local/remote machines, MagicDNS naming |
| **Daemon** | Rust daemon (per machine) | Manage tmux, worktrees, docker, cost tracking |
| **Persistence** | tmux | Sessions survive disconnects, auto-reattach on reconnect |
| **Communication** | PTY / tmux send-keys | Bridge between IDE and interactive agents |
| **Isolation** | Git Worktree + Docker | Code isolation + environment isolation |
| **Naming** | Portless | Each worktree gets a named URL |
| **Agent Runtime** | Claude Code / Aider / Codex | Pluggable — IDE does not bind to a specific agent |

## Tech Stack

### Client: Tauri 2.x

**Why Tauri over Electron:**
- Memory efficiency matters: users may have 5-10 terminal renderers + diff views open simultaneously
- Tauri's Rust backend handles all system interactions (tmux, pty, git, docker) natively

**Risk:** WebView cross-platform inconsistency (WebView2 on Windows, WKWebView on macOS, WebKitGTK on Linux). Requires extra cross-platform testing investment.

**Frontend:** React + xterm.js

### Session Persistence: tmux

The most reliable approach, confirmed by both community practice and our research.

**Design:**
- Each agent session = one tmux session
- Naming convention: `otte-{project}-{branch}`
- Daemon auto-creates/destroys tmux sessions, tied to worktree lifecycle
- Remote daemons connect via Tailscale SSH (passwordless)
- Client reconnects auto-reattach — seamless recovery

### Agent Communication: Three-Phase Strategy

This is the **highest technical complexity** in the architecture. A layered approach:

#### Phase 1 — MVP: tmux send-keys + capture-pane

```
IDE  --[tmux send-keys]--> tmux session --> Agent
IDE  <--[tmux capture-pane]-- tmux session
```

- Inject prompts: `tmux send-keys -t session-name "prompt" Enter`
- Read output: `tmux capture-pane -t session-name -p`
- **Pro:** Works with ANY terminal-based agent. Zero agent-specific code.
- **Con:** Output requires ANSI escape sequence parsing. No structured data.

#### Phase 2 — Mid-term: Direct PTY Bridging

```
IDE  --[pty master read/write]--> PTY --> Agent
IDE  --[xterm.js render]-->  User
```

- Use Rust `portable-pty` or Node `node-pty` for pseudo-terminal allocation
- Agent runs inside PTY, believes it has a real terminal
- IDE reads/writes via PTY master, renders via xterm.js
- **Pro:** Real-time bidirectional communication, high fidelity rendering
- **Con:** Must handle ANSI parsing, flow control, terminal resize sync

#### Phase 3 — Long-term: Agent SDK Integration

```
IDE  --[SDK API calls]--> Agent SDK --> Structured responses
```

- Use Claude Code Agent SDK to build custom interaction loops
- Get structured output, tool approval callbacks, native message objects
- Full control over agent behavior
- **Con:** High development cost, each agent needs separate adapter

**Key decision:** All three phases coexist via an **Agent Adapter** abstraction. Start with Phase 1 for universality, add Phase 2 for performance, add Phase 3 for the most popular agents.

### Environment Isolation

| Strategy | When to Use | When NOT to Use |
|----------|-------------|-----------------|
| **Bare Git Worktree** (default) | Lightweight projects, no env isolation needed | Multi-service, port conflicts, system-level deps |
| **Docker Sandbox** (opt-in) | Need isolation, want `--dangerously-skip-permissions` | Resource-constrained machines, no Docker installed |

**Not recommended for MVP:**
- Nix Flakes — learning curve too steep, narrows target audience
- Firecracker — overkill for individual developers

Unified via an **Environment Provider** abstraction layer.

### Networking: Tailscale + Portless

- Tailscale provides the mesh network between local and remote machines
- Portless assigns named URLs to worktree services
- **Cross-machine preview:** Use `Tailscale Serve` to expose Portless local addresses to the tailnet
- Result: `feature-auth.vps.tailnet` reaches the correct worktree's service from any machine

[Next: Session Lifecycle >](Session-Lifecycle)
