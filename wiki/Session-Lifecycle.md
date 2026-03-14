# Session Lifecycle

[< Home](Home.md) | [< Technical Architecture](Technical-Architecture.md)

## Repo Import Flow

Repos are first-class objects. Before creating sessions, the user imports a local git repo:

```
User clicks "Import Repo"
        |
        v
[1] Native folder picker opens (tauri-plugin-dialog)
        |
        v
[2] Backend validates .git directory exists
    - Rejects if already imported (duplicate check)
        |
        v
[3] Repo inserted into SQLite (~/.racc/racc.db)
    - Stores: id, path, name (derived from path), added_at
        |
        v
[4] Repo appears in sidebar, ready for agent sessions
```

## Session Creation Flow

Within an imported repo, users launch agent sessions via the NewAgentDialog:

```
User clicks [+] on a repo
        |
        v
[1] Configure session
    - Select agent (currently Claude Code)
    - Skip permissions checkbox (default: on) — appends --dangerously-skip-permissions
    - Choose: "Run in repo" or "Create worktree"
    - If worktree: provide branch name
        |
        v
[2] Environment Preparation
    - (If worktree) git worktree add at ~/racc-worktrees/{repo}/{branch}
        |
        v
[3] Session Persistence
    - Insert session record into SQLite with status "Running"
    - Store: repo_id, agent type, worktree_path, branch, timestamps
        |
        v
[4] PTY Spawn
    - ptyManager.spawnPty() creates native PTY process
    - Working directory set to worktree or repo path
    - Shell inherits user environment
        |
        v
[5] Agent Startup
    - After 100ms delay, agent command (e.g., "claude") sent to PTY stdin
        |
        v
[6] Communication Channel
    - usePtyBridge hook wires up:
      • PTY output → xterm.js rendering (with buffer for session switching)
      • xterm.js keyboard input → PTY stdin
      • Terminal resize → PTY resize
        |
        v
[7] State Registration
    - Session appears nested under repo in sidebar with green "Running" dot
    - Token usage monitoring begins via JSONL file polling (10s interval)
```

## State Machine

```
               +---v---+      +---v---+
               |Running|      | Error |
               +---+---+      +-------+
                   |
          +--------+--------+
          |                  |
     user stops          app closes /
     session             PTY dies
          |                  |
     +----v-----+    +------v------+
     |Completed |    |Disconnected |
     +----+-----+    +------+------+
          |                  |
          +--------+---------+
                   |
            user clicks ▶
            (reattach)
                   |
              +----v----+
              | Running |  (new PTY, claude --continue)
              +---------+
```

### State Definitions

| State | Meaning | Entry Trigger | User Can... |
|-------|---------|---------------|-------------|
| **Running** | Agent is actively executing in PTY | Session created or reattached, PTY spawned successfully | View terminal, send input, stop, remove |
| **Completed** | Session stopped by user | User clicks stop → PTY killed, DB updated | Reattach, remove session |
| **Disconnected** | PTY process no longer exists | App restart — reconciliation marks all previously Running sessions | Reattach, remove session |
| **Error** | Session creation or operation failed | PTY spawn failure / unexpected error | Remove, retry |

### Key Design: Reconciliation on Startup

On app startup, `reconcile_sessions()` handles the fact that PTY state is in-memory and lost on restart:

1. Query SQLite for sessions with status `Running`
2. Update ALL of them to `Disconnected` (PTY processes cannot survive app restart)
3. Return full repo + session list to frontend

**Tradeoff:** Unlike tmux-based sessions, PTY processes do not survive app crashes. However, `reattach_session` mitigates this by re-spawning the PTY with `claude --continue` to resume the conversation. Session immortality via remote execution is planned for v0.2.

### Session Reattach

Disconnected and Completed sessions can be reattached via the ▶ button in the sidebar:

1. Backend `reattach_session` validates the session is not already Running
2. If the session has a worktree, verifies the worktree directory still exists on disk
3. Updates session status to `Running` in SQLite
4. Frontend spawns a new PTY in the session's working directory (worktree path or repo path)
5. Sends `claude --continue` (with optional `--dangerously-skip-permissions`) to resume the last conversation in that directory

**Limitation:** Terminal output from the previous PTY is lost — only the Claude Code conversation context is resumed via `--continue`.

### Session Cleanup

- **Stop session:** kills PTY process via `ptyManager.killPty()`, updates SQLite status to `Completed`
- **Remove session:** confirmation dialog required; any session can be removed regardless of status
  - If the session is `Running`, the backend marks it as `Completed` first, then the frontend kills the PTY and stops tracking
  - If the session has a worktree, dialog shows an optional checkbox to also delete the worktree via `git worktree remove --force`
  - Deletes the SQLite session record and triggers batch analysis cleanup
- **Remove repo:** only allowed if no `Running` sessions; cascades to delete all session records
- **App close:** `killAll()` terminates all active PTY processes via window `beforeunload` event

### PTY Buffer Management

When switching between sessions, the terminal needs to display previous output:

- Each PTY accumulates output in a `Uint8Array[]` buffer managed by `ptyManager.ts`
- Maximum buffer size: **1MB per session** (oldest chunks dropped when exceeded)
- On session switch, `usePtyBridge` replays the buffer into xterm.js
- Live output continues streaming after replay completes

[Next: Competitive Analysis >](Competitive-Analysis.md)
