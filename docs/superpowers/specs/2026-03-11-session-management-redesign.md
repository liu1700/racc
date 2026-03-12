# Session Management Redesign — Design Spec

**Date:** 2026-03-11
**Status:** Approved

## Goal

Replace the current ephemeral session management (tmux-only, metadata lost on reload) with a persistent SQLite-backed model where repos are first-class objects and agent sessions are nested within them.

## Data Model

### SQLite Tables

```sql
CREATE TABLE repos (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    path TEXT NOT NULL UNIQUE,
    name TEXT NOT NULL,
    added_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE sessions (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    repo_id INTEGER NOT NULL REFERENCES repos(id) ON DELETE CASCADE,
    tmux_session_name TEXT NOT NULL UNIQUE,
    agent TEXT NOT NULL DEFAULT 'claude-code',
    worktree_path TEXT,          -- null if running directly in repo
    branch TEXT,                 -- branch name if worktree was created
    status TEXT NOT NULL DEFAULT 'Running',  -- Running | Completed | Disconnected | Error
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);
```

### Reconciliation on Startup

On app start, `reconcile_sessions()`:
1. Query all sessions with status `Running` from SQLite
2. For each, check if `tmux_session_name` exists in live tmux (`tmux has-session -t <name>`)
3. If tmux session is gone → update status to `Disconnected`
4. If tmux session exists → keep as `Running`
5. Return reconciled list to frontend

## UI Flow

### Sidebar Layout

```
┌─────────────────────┐
│  + Import Repo      │  ← Finder folder picker
├─────────────────────┤
│ ▼ my-app        [+] │  ← repo row (expandable, [+] launches agent)
│   ● claude-code  ■  │  ← session (● = status dot, ■ = stop)
│   ○ claude-code  ×  │  ← disconnected session (× = remove)
├─────────────────────┤
│ ▶ other-project [+] │  ← collapsed repo
└─────────────────────┘
```

- **"+ Import Repo" button** — triggers `tauri-plugin-dialog` native folder picker
- **Repo row** — shows repo name, expand/collapse, [+] button to launch new agent
- **Session row** — shows agent type, status dot, stop/remove button
- **Active session** — highlighted, clicking connects terminal to its tmux pane

### Import Repo Flow

1. User clicks "+ Import Repo"
2. Native Finder dialog opens (`dialog.open({ directory: true })`)
3. User selects a folder
4. Backend validates: folder exists, contains `.git` directory
5. If valid: insert into SQLite, return `Repo` object, sidebar updates
6. If invalid: show error "Not a git repository"
7. If already imported: show error "Repository already imported"

### Launch Agent Flow

1. User clicks [+] on a repo row
2. Small dialog appears:
   - Radio: **"Run in repo"** (default) / **"Create worktree"**
   - If "Create worktree" selected: branch name text input appears
   - Submit button: "Launch"
3. Backend:
   - If worktree: `git worktree add` at `~/racc-worktrees/<repo-name>/<branch>`
   - Create tmux session named `racc::<repo-name>::<branch>` (worktree) or `racc::<repo-name>::<current-branch>` (direct, detected via `git rev-parse --abbrev-ref HEAD`)
   - Send keys to start `claude` in the session
   - Insert session record into SQLite
4. Session appears nested under repo in sidebar

### Stop / Remove

- **Stop session:** kills tmux session, updates SQLite status to `Completed`, worktree is kept
- **Remove disconnected/completed session:** deletes SQLite record, optionally cleans up worktree
- **Remove repo:** only allowed if no `Running` sessions. Disconnected/Completed/Error sessions are silently deleted via `ON DELETE CASCADE`. Does NOT delete the repo from disk or any worktrees.

## Backend Architecture

### New Module: `src-tauri/src/commands/db.rs`

Database initialization and migrations using `rusqlite`.

```rust
pub fn init_db() -> Result<Connection, String>
```

- Opens/creates `~/.racc/racc.db`
- Sets `PRAGMA foreign_keys = ON` (required for `ON DELETE CASCADE`)
- Runs schema migrations using `PRAGMA user_version` to track schema version:
  - Version 0 → 1: CREATE TABLE repos + sessions
  - Future migrations increment `user_version` and apply ALTER TABLE statements
- Returns connection

The `Connection` should be managed as Tauri app state (`tauri::State<std::sync::Mutex<Connection>>`). The `std::sync::Mutex` is appropriate here because rusqlite operations are fast (sub-millisecond for typical queries) and the lock is held only briefly. All Tauri commands acquire the lock, perform the DB operation, and release it before any async work (tmux commands).

### Rewritten: `src-tauri/src/commands/session.rs`

All session operations now go through SQLite + tmux.

**New commands:**

| Command | Signature | Behavior |
|---------|-----------|----------|
| `import_repo` | `(path: String) -> Result<Repo, String>` | Validate `.git` exists, check not duplicate, insert into SQLite |
| `list_repos` | `() -> Result<Vec<RepoWithSessions>, String>` | Return all repos with nested sessions from SQLite |
| `remove_repo` | `(repo_id: i64) -> Result<(), String>` | Check no running sessions, delete repo + sessions from SQLite |
| `create_session` | `(repo_id: i64, use_worktree: bool, branch: Option<String>) -> Result<Session, String>` | Look up repo path, optionally create worktree, create tmux session, insert into SQLite |
| `stop_session` | `(session_id: i64) -> Result<(), String>` | Kill tmux session, update SQLite status to `Completed` |
| `remove_session` | `(session_id: i64) -> Result<(), String>` | Delete session record (only if not Running) |
| `reconcile_sessions` | `() -> Result<Vec<RepoWithSessions>, String>` | Check all Running sessions against tmux, update stale ones, return full state |

### Rust Types

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SessionStatus {
    Running,
    Completed,
    Disconnected,
    Error,
}

#[derive(Debug, Clone, Serialize)]
pub struct Repo {
    pub id: i64,
    pub path: String,
    pub name: String,
    pub added_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Session {
    pub id: i64,
    pub repo_id: i64,
    pub tmux_session_name: String,
    pub agent: String,
    pub worktree_path: Option<String>,
    pub branch: Option<String>,
    pub status: SessionStatus,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct RepoWithSessions {
    pub repo: Repo,
    pub sessions: Vec<Session>,
}
```

`SessionStatus` is stored as TEXT in SQLite (e.g., `"Running"`). The old 7-variant enum (`Creating`, `Waiting`, `Paused` etc.) is reduced to 4 variants because the removed states were never used in practice. The Tailwind status colors for removed states (`status-waiting`, `status-paused`) should be cleaned up in `tailwind.config.ts`.

### Dependencies

Add to `src-tauri/Cargo.toml`:
- `rusqlite = { version = "0.31", features = ["bundled"] }` — SQLite with bundled library
- `tauri-plugin-dialog = "2"` — native file/folder picker

Create `src-tauri/capabilities/default.json` if it doesn't exist, and add:
- `"dialog:default"` — permission for dialog plugin
- Include existing permissions (shell, etc.)

Register dialog plugin in `lib.rs`:
- `.plugin(tauri_plugin_dialog::init())`

## Frontend Architecture

### Types — `src/types/session.ts`

Replace current types with:

```typescript
export interface Repo {
  id: number;
  path: string;
  name: string;
  added_at: string;
}

export type SessionStatus = "Running" | "Completed" | "Disconnected" | "Error";

export interface Session {
  id: number;
  repo_id: number;
  tmux_session_name: string;
  agent: string;
  worktree_path: string | null;
  branch: string | null;
  status: SessionStatus;
  created_at: string;
}

export interface RepoWithSessions {
  repo: Repo;
  sessions: Session[];
}
```

### Store — `src/stores/sessionStore.ts`

Rewrite to manage repos + sessions:

```typescript
interface SessionState {
  repos: RepoWithSessions[];
  activeSessionId: number | null;
  loading: boolean;
  error: string | null;

  // Derived selector — used by Terminal (needs tmux_session_name) and CostTracker (needs worktree_path + repo.path)
  getActiveSession: () => { session: Session; repo: Repo } | null;

  initialize: () => Promise<void>;       // reconcile_sessions on mount
  importRepo: (path: string) => Promise<void>;
  removeRepo: (repoId: number) => Promise<void>;
  createSession: (repoId: number, useWorktree: boolean, branch?: string) => Promise<void>;
  stopSession: (sessionId: number) => Promise<void>;
  removeSession: (sessionId: number) => Promise<void>;
  setActiveSession: (id: number) => void;
  clearError: () => void;
}
```

**Terminal integration:** `Terminal.tsx` / `useTmuxBridge` uses `getActiveSession()?.session.tmux_session_name` instead of `activeSessionId` directly.

**CostTracker integration:** uses `getActiveSession()` and passes `session.worktree_path ?? repo.path` to `get_project_costs`.

**Error handling convention:** all async actions catch errors and set `error: string | null` in the store. Components read `error` to display. `clearError()` resets it.

### Components

**Replace `NewSessionDialog.tsx` with two new components:**

1. **`ImportRepoDialog.tsx`** — triggered by "+ Import Repo" button
   - Calls `open()` from `@tauri-apps/plugin-dialog` with `{ directory: true }`
   - Invokes `import_repo` with selected path
   - Shows error if not a git repo or already imported

2. **`NewAgentDialog.tsx`** — triggered by [+] on a repo row
   - Props: `repoId: number`
   - Radio: "Run in repo" / "Create worktree"
   - Conditional branch name input
   - Invokes `create_session`

**Rewrite `Sidebar.tsx`:**
- Render repos as expandable groups
- Sessions nested under their repo
- Import repo button at top
- Per-repo [+] button to launch agents

### App.tsx Changes

- On mount: call `initialize()` (which calls `reconcile_sessions`) instead of `fetchSessions()`
- Remove the `setInterval(fetchSessions, 5000)` — SQLite is the source of truth, no need to poll tmux. Only reconcile on mount.

## Cost Tracker Integration

The `get_project_costs(worktree_path)` command still works unchanged. `CostTracker` uses the `getActiveSession()` selector from the store:

```typescript
const active = useSessionStore((s) => s.getActiveSession());
const costPath = active?.session.worktree_path ?? active?.repo.path;
// pass costPath to invoke("get_project_costs", { worktreePath: costPath })
```

## What's NOT in Scope

- Session history / activity log persistence
- Worktree cleanup on session removal (manual for now)
- Multiple agent types (Claude Code only)
- Remote repo support (local only)
- Repo settings / configuration

## Success Criteria

- Repos persist across app restarts
- Sessions persist with correct metadata (agent, worktree path, status)
- Dead tmux sessions are detected and marked on app start
- Native Finder picker works for repo import
- Only valid git repos can be imported
- Agent sessions can run directly in repo or in a new worktree
- Sidebar shows repos with nested sessions
- Existing cost tracking continues to work
