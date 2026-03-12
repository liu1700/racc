# Session Lifecycle

[< Home](Home.md) | [< Technical Architecture](Technical-Architecture.md)

## Repo Import Flow

Repos are first-class objects. Before creating sessions, the user imports a local git repo:

```
User clicks "Import Repo"
        |
        v
[1] Native Finder dialog opens (tauri-plugin-dialog)
        |
        v
[2] Backend validates .git directory exists
        |
        v
[3] Repo inserted into SQLite (~/.racc/racc.db)
        |
        v
[4] Repo appears in sidebar, ready for agent sessions
```

## Session Creation Flow

Within an imported repo, users launch agent sessions:

```
User clicks [+] on a repo
        |
        v
[1] Choose mode: "Run in repo" or "Create worktree"
    - If worktree: provide branch name
        |
        v
[2] Environment Preparation
    - (If worktree) git worktree add at ~/racc-worktrees/{repo}/{branch}
    - (If direct) detect current branch via git rev-parse
        |
        v
[3] Session Persistence
    - Create named tmux session: racc::{repo-name}::{branch}
    - Set working directory to worktree or repo path
    - Insert session record into SQLite
        |
        v
[4] Agent Startup
    - Launch Claude Code inside tmux session via send-keys
        |
        v
[5] Communication Channel
    - Establish IDE <-> tmux send-keys/capture-pane channel
        |
        v
[6] State Registration
    - Session appears nested under repo in sidebar
    - Begin cost monitoring via Claude Code JSONL parsing
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
     session             tmux dies
          |                  |
     +----v-----+    +------v------+
     |Completed |    |Disconnected |
     +----------+    +------+------+
                            |
                     app restarts,
                     reconcile_sessions()
                     detects dead tmux
```

### State Definitions

| State | Meaning | Entry Trigger | User Can... |
|-------|---------|---------------|-------------|
| **Running** | Agent is actively executing in tmux | Session created successfully | View terminal, stop |
| **Completed** | Session stopped by user | User clicks stop | Remove session record |
| **Disconnected** | tmux session no longer exists | App restart detects dead tmux via reconciliation | Remove session record |
| **Error** | Session creation or operation failed | Process crash / non-zero exit | Remove, retry |

### Key Design: Reconciliation on Startup

On app startup, `reconcile_sessions()` checks all `Running` sessions against live tmux state:
1. Query SQLite for sessions with status `Running`
2. For each, run `tmux has-session -t <name>`
3. If tmux session is gone → update status to `Disconnected` in SQLite
4. Return full repo + session list to frontend

This ensures the UI always reflects reality, even after crashes or restarts.

### Session Cleanup

- **Stop session:** kills tmux, updates SQLite status to `Completed`, worktree is kept
- **Remove session:** deletes SQLite record (only if not `Running`)
- **Remove repo:** only allowed if no `Running` sessions; cascades to delete all session records

[Next: Competitive Analysis >](Competitive-Analysis.md)
