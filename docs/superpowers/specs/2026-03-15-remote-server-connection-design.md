# Remote Server Connection Design

**Date:** 2026-03-15
**Status:** Draft

## Overview

Add remote server support to Racc: users connect to remote servers via SSH, run an AI-driven setup wizard to provision the environment, and then run AI coding agent sessions (Claude Code, Codex, or custom) inside tmux on the remote server. The terminal experience is seamless — remote sessions look and behave identically to local sessions in xterm.js.

## Design Decisions

| Decision | Choice |
|----------|--------|
| SSH config | Hybrid — read `~/.ssh/config` + manual input |
| Terminal output | Reuse existing xterm.js, seamless local/remote |
| Setup flow | AI agent-driven via `pi-agent-core` (semi-auto, user confirms before installing) |
| Code management | Racc clones repos on remote; git access is part of setup |
| tmux strategy | One tmux session per Racc session (`racc-{session_id}`) |
| Reconnection | Auto-reconnect with exponential backoff, tmux reattach |
| Agent scope | Agent-agnostic with Claude Code / Codex presets |
| SSH implementation | Rust-side via `russh` crate |
| Architecture | Transport abstraction layer — local PTY and SSH are both implementations of a `Transport` trait |

## Data Model

### `servers` Table

```sql
CREATE TABLE servers (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    host TEXT NOT NULL,
    port INTEGER DEFAULT 22,
    username TEXT NOT NULL,
    auth_method TEXT NOT NULL,    -- "key" | "ssh_config" | "agent"
    key_path TEXT,
    ssh_config_host TEXT,
    setup_status TEXT DEFAULT 'pending',  -- "pending" | "ready" | "partial" | "error"
    setup_details TEXT,           -- JSON: agent's final environment report
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
```

### `sessions` Table Change

```sql
ALTER TABLE sessions ADD COLUMN server_id TEXT;  -- null = local session
```

No foreign key. Plain field. Frontend/backend uses this to determine session type.

### Agent Profiles

Not stored in DB. Defined as Rust config:

```rust
struct AgentProfile {
    name: String,           // "claude-code", "codex", "custom"
    install_check: String,  // "which claude"
    install_cmd: String,    // "npm install -g @anthropic-ai/claude-code"
    launch_cmd: String,     // "claude"
}
```

## Transport Abstraction Layer

### Transport Trait

```rust
#[async_trait]
pub trait Transport: Send + Sync {
    async fn write(&self, data: &[u8]) -> Result<(), TransportError>;
    async fn read(&self) -> Result<Vec<u8>, TransportError>;
    async fn resize(&self, cols: u16, rows: u16) -> Result<(), TransportError>;
    async fn close(&self) -> Result<(), TransportError>;
    fn is_alive(&self) -> bool;
}
```

### LocalPtyTransport

Wraps existing `tauri-plugin-pty`:

- `write()` → PTY stdin
- `read()` → PTY stdout
- `resize()` → PTY resize
- `close()` → kill process

### SshTmuxTransport

Wraps `russh` SSH channel:

- On create: `ssh → tmux new-session -d -s racc-{session_id} '{agent_cmd}'` then `tmux attach`
- `write()` → SSH channel stdin (forwarded to tmux)
- `read()` → SSH channel stdout (tmux output stream)
- `resize()` → SSH channel window size change
- `close()` → `tmux kill-session -t racc-{session_id}`
- On disconnect: auto SSH reconnect → `tmux attach -t racc-{session_id}`

### TransportManager

```rust
pub struct TransportManager {
    transports: HashMap<String, Box<dyn Transport>>,  // session_id → transport
}

impl TransportManager {
    pub async fn create_local(&self, session_id: &str, cwd: &str, cmd: &str) -> Result<()>;
    pub async fn create_remote(&self, session_id: &str, server: &Server, cmd: &str) -> Result<()>;
    pub async fn get(&self, session_id: &str) -> Option<&dyn Transport>;
    pub async fn remove(&self, session_id: &str) -> Result<()>;
}
```

Injected as Tauri managed state alongside `EventSender` and `DbPool`.

### Tauri Commands

```rust
#[tauri::command]
async fn transport_write(session_id: String, data: Vec<u8>, state: State<TransportManager>) -> Result<()>;

#[tauri::command]
async fn transport_resize(session_id: String, cols: u16, rows: u16, state: State<TransportManager>) -> Result<()>;
```

Read direction: transport background task continuously reads and pushes data via `app_handle.emit_to()` to the frontend.

### Impact on Existing Code

- `ptyManager.ts` — replaced entirely. Frontend no longer calls `tauri-plugin-pty` directly; instead calls transport commands.
- `usePtyBridge.ts` — simplified to xterm.js ↔ Tauri event/command bridge, agnostic of local vs remote.
- `session.rs` `create_session` — routes to different transport creation based on `server_id`.

## SSH Connection Management

### SshManager

```rust
pub struct SshManager {
    connections: HashMap<String, SshConnection>,  // server_id → connection
}

pub struct SshConnection {
    client: russh::client::Handle,
    server_config: Server,
    status: ConnectionStatus,  // Connected | Disconnected | Reconnecting
}
```

Injected as Tauri managed state.

### Connection Lifecycle

```
add_server → connect → setup agent → ready
                ↓ (disconnect)
           auto reconnect (1s, 2s, 4s, 8s, 16s backoff, max 5 attempts)
                ↓ (all failed)
           Disconnected status, UI shows manual reconnect button
```

### SSH Config Parsing

Connection priority:

1. `auth_method = "ssh_config"` → parse `~/.ssh/config` via `ssh2-config` crate
2. `auth_method = "agent"` → system ssh-agent authentication
3. `auth_method = "key"` → specified key file

### Auto-Reconnect

When transport `read()` detects SSH channel down:

1. Notify frontend: "Reconnecting..." status
2. `SshManager` initiates reconnect with exponential backoff
3. On success → `tmux attach -t racc-{session_id}`
4. Resume output stream, frontend seamless transition
5. After 5 failures → mark Disconnected, emit event to frontend

### Server Tauri Commands

```rust
#[tauri::command]
async fn add_server(config: ServerConfig) -> Result<Server>;

#[tauri::command]
async fn remove_server(server_id: String) -> Result<()>;

#[tauri::command]
async fn list_servers() -> Result<Vec<Server>>;

#[tauri::command]
async fn test_connection(server_id: String) -> Result<ConnectionTestResult>;

#[tauri::command]
async fn connect_server(server_id: String) -> Result<()>;

#[tauri::command]
async fn disconnect_server(server_id: String) -> Result<()>;
```

## AI-Driven Setup Flow

### Architecture

Instead of hardcoded detection chains and install commands, the setup is driven by a `@mariozechner/pi-agent-core` agent running locally. The agent uses tools to execute commands on the remote server via `SshManager`.

### Setup Agent Definition

```typescript
const setupAgent = new Agent({
  initialState: {
    systemPrompt: `You are a server setup assistant for Racc.
Your job is to prepare a remote server for running AI coding agents via tmux.

You have SSH access to the server. Assess the environment and guide the user:
1. Check OS, package manager, available tools
2. Ensure git is installed and can access repositories
3. Ensure tmux is installed
4. For each requested agent (claude-code, codex, etc.):
   - Check if installed
   - If installed, PRIORITIZE login/authentication setup first
   - If not installed, offer to install
5. Adapt to the server's OS and package manager

Always ask for user confirmation before installing or modifying anything.
Provide clear guidance for steps requiring manual action.`,
    model: getModel("anthropic", "claude-sonnet-4-20250514"),
    tools: [runRemoteCommand, getServerInfo],
  },
});
```

### Agent Tools

```typescript
const runRemoteCommand: AgentTool = {
  name: "run_remote_command",
  description: "Execute a command on the remote server via SSH",
  parameters: {
    command: { type: "string" },
    requires_confirmation: { type: "boolean" },
  },
  // Executes via SshManager, streams output to frontend
};

const getServerInfo: AgentTool = {
  name: "get_server_info",
  description: "Get known info about this server",
  // Returns server config and known environment info
};
```

### Agent Setup Priority for AI Tools

For Claude Code, Codex, and similar agents, the setup agent prioritizes:

1. **Check installation** — `which claude` / `which codex`
2. **If installed → prioritize login/auth first** — prompt user to run `claude login` or set API key
3. **Verify auth works** — test that the agent can actually run
4. **If not installed → offer to install** — with user confirmation

### Frontend UI

The setup wizard is a conversational interface, not a static checklist:

```
┌─ Server Setup: GPU Box ──────────────────┐
│                                           │
│ 🤖 Let me check your server environment..│
│                                           │
│ > Running: uname -a                       │
│   Ubuntu 22.04 LTS, x86_64               │
│                                           │
│ 🤖 Git is installed but can't access      │
│    GitHub. You'll need to either:         │
│    1. Generate an SSH key on the server   │
│    2. Or use a personal access token      │
│    Which would you prefer?                │
│                                           │
│ User: [input field                     ]  │
│                                    [Send] │
└───────────────────────────────────────────┘
```

### Advantages Over Hardcoded Setup

- **Adaptive** — handles any OS, package manager, network config
- **Conversational** — user can ask questions, agent gives targeted advice
- **Extensible** — add new agents by updating system prompt, no code changes
- **Fault-tolerant** — agent can diagnose unexpected issues autonomously

## Frontend UI

### Sidebar — Servers Section

```
┌─ Sidebar ──────────────────┐
│ SERVERS                     │
│  🖥 GPU Box    ● connected  │
│  🖥 Dev VM     ○ disconnected│
│  [+ Add Server]            │
│ ────────────────────────── │
│ SESSIONS                    │
│  ▸ local-session-1         │
│  ▸ remote-session-1 (GPU Box) │
└────────────────────────────┘
```

Remote sessions show server name. Otherwise identical to local sessions.

### Add Server Dialog

```
┌─ Add Server ──────────────────────┐
│ Name: [                     ]     │
│                                    │
│ Connection Method:                 │
│  ○ From SSH Config  ○ Manual      │
│                                    │
│ [SSH Config mode]                  │
│ Host Alias: [ dropdown ]          │
│                                    │
│ [Manual mode]                      │
│ Host: [                  ]        │
│ Port: [22                ]        │
│ Username: [              ]        │
│ Auth: ○ SSH Key  ○ SSH Agent      │
│ Key Path: [              ] [📁]   │
│                                    │
│      [Test Connection] [Add]      │
└────────────────────────────────────┘
```

### Remote Session Creation

Existing "Fire Task" flow extended with server selection:

```
Server: [Local] [GPU Box] [Dev VM]
```

After selecting a remote server, the rest of the flow is unchanged (repo, task, agent). Racc handles remote clone/worktree, then starts agent in tmux.

### Status Bar

```
GPU Box: ● connected | Dev VM: ○ reconnecting (2/5)
```

## End-to-End Data Flow

```
┌─ Racc App ──────────────────────────────────────────┐
│                                                      │
│  xterm.js ←─ Tauri events ─← TransportManager      │
│     │                            │                   │
│     └─ Tauri commands ──→  ┌─────┴──────┐           │
│        (write/resize)      │ Transport   │           │
│                            │  trait      │           │
│                     ┌──────┴──────┬──────┘           │
│                     ▼             ▼                   │
│              LocalPty      SshTmux                   │
│              Transport     Transport                 │
│                 │             │                       │
│           tauri-plugin-pty   SshManager               │
│                 │             │                       │
│            local PTY    ┌────┴────┐                  │
│                         │  russh  │                  │
│                         └────┬────┘                  │
└──────────────────────────────│───────────────────────┘
                               │ SSH
                    ┌──────────▼──────────┐
                    │   Remote Server      │
                    │  tmux session        │
                    │   └─ claude / codex  │
                    │  git worktree        │
                    └──────────────────────┘
```

## Phased Delivery

### Phase 1: Infrastructure

- Define `Transport` trait + implement `LocalPtyTransport` (refactor existing PTY)
- `TransportManager` as Tauri managed state
- Migrate frontend to transport commands (replace direct PTY plugin calls)
- **Exit criteria:** Local sessions fully equivalent, running on new architecture

### Phase 2: Server Management

- `servers` table + CRUD commands
- `SshManager` (russh connection pool, auto-reconnect)
- Frontend: Add Server dialog, Servers list in sidebar
- SSH config parsing via `ssh2-config` crate

### Phase 3: Setup Agent

- Integrate `@mariozechner/pi-agent-core` and `@mariozechner/pi-ai`
- Setup Agent with tools: `run_remote_command`, `get_server_info`
- Frontend: conversational setup wizard UI
- Remote environment detection and guided installation

### Phase 4: Remote Sessions

- `SshTmuxTransport` implementation
- Remote git clone / worktree management
- Full remote session creation flow
- Auto-reconnect + tmux reattach on disconnect

### Phase 5: Polish

- Status bar connection status display
- Remote session reattach after Racc restart
- Remote cost tracking
- Concurrent sessions across multiple servers

## Technology Choices

| Component | Choice | Rationale |
|-----------|--------|-----------|
| SSH library | `russh` | Pure Rust, async, actively maintained |
| SSH config parsing | `ssh2-config` crate | Parse `~/.ssh/config` for host aliases |
| Setup Agent | `@mariozechner/pi-agent-core` + `pi-ai` | Lightweight agent runtime with tool calling |
| tmux management | SSH exec commands | Simple, reliable, no additional dependencies on remote |
| Terminal streaming | SSH channel → Tauri event emit → xterm.js | Consistent with existing local PTY data flow |
| Database | Existing SQLite, new `servers` table | No new infrastructure |
