# Session Management Redesign Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace ephemeral tmux-only session management with SQLite-backed persistence, first-class repo objects, and native folder picker for importing repos.

**Architecture:** SQLite database at `~/.racc/racc.db` stores repos and sessions. Rust backend performs all CRUD through SQLite + tmux operations. React frontend uses Zustand store with `getActiveSession()` selector for Terminal/CostTracker integration. Native Finder dialog (tauri-plugin-dialog) for repo import.

**Tech Stack:** Rust (rusqlite, serde, tauri-plugin-dialog), Tauri 2.x IPC, React 19, TypeScript, Zustand

**Spec:** `docs/superpowers/specs/2026-03-11-session-management-redesign.md`

---

## File Map

| Action | File | Responsibility |
|--------|------|---------------|
| Modify | `src-tauri/Cargo.toml` | Add rusqlite + tauri-plugin-dialog dependencies |
| Create | `src-tauri/src/commands/db.rs` | SQLite init, migrations, connection management |
| Rewrite | `src-tauri/src/commands/session.rs` | Types (Repo, Session, SessionStatus) + all Tauri commands (import_repo, create_session, etc.) |
| Modify | `src-tauri/src/commands/mod.rs` | Add `pub mod db;` |
| Modify | `src-tauri/src/lib.rs` | Register DB state, dialog plugin, new commands |
| Rewrite | `src/types/session.ts` | Repo, Session, SessionStatus, RepoWithSessions TypeScript types |
| Rewrite | `src/stores/sessionStore.ts` | Repo + session Zustand store with getActiveSession() |
| Create | `src/components/Sidebar/ImportRepoDialog.tsx` | Finder folder picker + import_repo invocation |
| Create | `src/components/Sidebar/NewAgentDialog.tsx` | Worktree choice radio + create_session invocation |
| Rewrite | `src/components/Sidebar/Sidebar.tsx` | Repo-grouped layout with nested sessions |
| Delete | `src/components/Sidebar/NewSessionDialog.tsx` | Replaced by ImportRepoDialog + NewAgentDialog |
| Modify | `src/components/Terminal/Terminal.tsx:9` | Use getActiveSession()?.session.tmux_session_name |
| Modify | `src/hooks/useTmuxBridge.ts` | No change needed — already takes sessionId: string |
| Modify | `src/components/CostTracker/CostTracker.tsx` | Use getActiveSession() for worktree_path ?? repo.path |
| Modify | `src/components/Dashboard/StatusBar.tsx` | Update for nested RepoWithSessions model |
| Modify | `src/App.tsx` | Call initialize() instead of fetchSessions(), remove polling interval |
| Modify | `tailwind.config.ts:18-25` | Remove unused status-waiting, status-paused colors |

---

## Chunk 1: Backend — Database + Session Module

### Task 1: Add dependencies to Cargo.toml

**Files:**
- Modify: `src-tauri/Cargo.toml`

- [ ] **Step 1: Add rusqlite and tauri-plugin-dialog to dependencies**

In `src-tauri/Cargo.toml`, add to `[dependencies]`:

```toml
rusqlite = { version = "0.31", features = ["bundled"] }
tauri-plugin-dialog = "2"
```

- [ ] **Step 2: Install frontend dialog plugin package**

Run: `bun add @tauri-apps/plugin-dialog`

- [ ] **Step 3: Verify cargo check passes**

Run: `cd src-tauri && cargo check`

- [ ] **Step 4: Commit**

```bash
git add src-tauri/Cargo.toml bun.lockb package.json
git commit -m "chore: add rusqlite and tauri-plugin-dialog dependencies"
```

---

### Task 2: Create database module (db.rs)

**Files:**
- Create: `src-tauri/src/commands/db.rs`
- Modify: `src-tauri/src/commands/mod.rs`

- [ ] **Step 1: Create `src-tauri/src/commands/db.rs`**

```rust
use rusqlite::Connection;
use std::fs;
use std::path::PathBuf;

/// Returns the path to the Racc database: ~/.racc/racc.db
fn db_path() -> Result<PathBuf, String> {
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or("Could not find home directory")?;
    let dir = home.join(".racc");
    fs::create_dir_all(&dir).map_err(|e| format!("Failed to create ~/.racc: {e}"))?;
    Ok(dir.join("racc.db"))
}

/// Initialize the database, run migrations, return the connection.
pub fn init_db() -> Result<Connection, String> {
    let path = db_path()?;
    let conn = Connection::open(&path).map_err(|e| format!("Failed to open database: {e}"))?;

    // Required for ON DELETE CASCADE
    conn.execute_batch("PRAGMA foreign_keys = ON;")
        .map_err(|e| format!("Failed to enable foreign keys: {e}"))?;

    // Migration system using user_version
    let version: i32 = conn
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .map_err(|e| format!("Failed to read user_version: {e}"))?;

    if version < 1 {
        conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS repos (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                path TEXT NOT NULL UNIQUE,
                name TEXT NOT NULL,
                added_at TEXT NOT NULL DEFAULT (datetime('now'))
            );

            CREATE TABLE IF NOT EXISTS sessions (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                repo_id INTEGER NOT NULL REFERENCES repos(id) ON DELETE CASCADE,
                tmux_session_name TEXT NOT NULL UNIQUE,
                agent TEXT NOT NULL DEFAULT 'claude-code',
                worktree_path TEXT,
                branch TEXT,
                status TEXT NOT NULL DEFAULT 'Running',
                created_at TEXT NOT NULL DEFAULT (datetime('now')),
                updated_at TEXT NOT NULL DEFAULT (datetime('now'))
            );

            PRAGMA user_version = 1;
            ",
        )
        .map_err(|e| format!("Migration v1 failed: {e}"))?;
    }

    Ok(conn)
}
```

- [ ] **Step 2: Add `pub mod db;` to `src-tauri/src/commands/mod.rs`**

Add the line `pub mod db;` after `pub mod cost;`.

- [ ] **Step 3: Verify cargo check passes**

Run: `cd src-tauri && cargo check`

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/commands/db.rs src-tauri/src/commands/mod.rs
git commit -m "feat(db): add SQLite database module with migration support"
```

---

### Task 3: Rewrite session.rs with SQLite-backed commands

**Files:**
- Rewrite: `src-tauri/src/commands/session.rs`

- [ ] **Step 1: Replace the entire `src-tauri/src/commands/session.rs`**

```rust
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::process::Command;
use std::sync::Mutex;

// --- Types ---

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SessionStatus {
    Running,
    Completed,
    Disconnected,
    Error,
}

impl SessionStatus {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Running => "Running",
            Self::Completed => "Completed",
            Self::Disconnected => "Disconnected",
            Self::Error => "Error",
        }
    }

    fn from_str(s: &str) -> Self {
        match s {
            "Running" => Self::Running,
            "Completed" => Self::Completed,
            "Disconnected" => Self::Disconnected,
            _ => Self::Error,
        }
    }
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

// --- Helper: query repos with sessions ---

fn query_repos_with_sessions(conn: &Connection) -> Result<Vec<RepoWithSessions>, String> {
    let mut repo_stmt = conn
        .prepare("SELECT id, path, name, added_at FROM repos ORDER BY name")
        .map_err(|e| e.to_string())?;

    let repos: Vec<Repo> = repo_stmt
        .query_map([], |row| {
            Ok(Repo {
                id: row.get(0)?,
                path: row.get(1)?,
                name: row.get(2)?,
                added_at: row.get(3)?,
            })
        })
        .map_err(|e| e.to_string())?
        .filter_map(|r| r.ok())
        .collect();

    let mut session_stmt = conn
        .prepare(
            "SELECT id, repo_id, tmux_session_name, agent, worktree_path, branch, status, created_at, updated_at
             FROM sessions WHERE repo_id = ? ORDER BY created_at DESC",
        )
        .map_err(|e| e.to_string())?;

    let mut result = Vec::new();
    for repo in repos {
        let sessions: Vec<Session> = session_stmt
            .query_map([repo.id], |row| {
                let status_str: String = row.get(6)?;
                Ok(Session {
                    id: row.get(0)?,
                    repo_id: row.get(1)?,
                    tmux_session_name: row.get(2)?,
                    agent: row.get(3)?,
                    worktree_path: row.get(4)?,
                    branch: row.get(5)?,
                    status: SessionStatus::from_str(&status_str),
                    created_at: row.get(7)?,
                    updated_at: row.get(8)?,
                })
            })
            .map_err(|e| e.to_string())?
            .filter_map(|r| r.ok())
            .collect();

        result.push(RepoWithSessions { repo, sessions });
    }

    Ok(result)
}

// --- Helper: check tmux session exists ---

fn tmux_session_exists(name: &str) -> bool {
    Command::new("tmux")
        .args(["has-session", "-t", name])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

// --- Helper: get current git branch ---

fn get_current_branch(repo_path: &str) -> Result<String, String> {
    let output = Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .current_dir(repo_path)
        .output()
        .map_err(|e| format!("Failed to get branch: {e}"))?;

    if !output.status.success() {
        return Err("Failed to detect current branch".to_string());
    }

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

// --- Tauri Commands ---

#[tauri::command]
pub async fn import_repo(
    db: tauri::State<'_, Mutex<Connection>>,
    path: String,
) -> Result<Repo, String> {
    // Validate .git exists
    let git_dir = std::path::Path::new(&path).join(".git");
    if !git_dir.exists() {
        return Err("Not a git repository".to_string());
    }

    let name = std::path::Path::new(&path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unknown")
        .to_string();

    let conn = db.lock().map_err(|e| e.to_string())?;

    conn.execute(
        "INSERT INTO repos (path, name) VALUES (?1, ?2)",
        rusqlite::params![path, name],
    )
    .map_err(|e| {
        if e.to_string().contains("UNIQUE") {
            "Repository already imported".to_string()
        } else {
            e.to_string()
        }
    })?;

    let id = conn.last_insert_rowid();
    let added_at: String = conn
        .query_row("SELECT added_at FROM repos WHERE id = ?1", [id], |row| {
            row.get(0)
        })
        .map_err(|e| e.to_string())?;

    Ok(Repo {
        id,
        path,
        name,
        added_at,
    })
}

#[tauri::command]
pub async fn list_repos(
    db: tauri::State<'_, Mutex<Connection>>,
) -> Result<Vec<RepoWithSessions>, String> {
    let conn = db.lock().map_err(|e| e.to_string())?;
    query_repos_with_sessions(&conn)
}

#[tauri::command]
pub async fn remove_repo(
    db: tauri::State<'_, Mutex<Connection>>,
    repo_id: i64,
) -> Result<(), String> {
    let conn = db.lock().map_err(|e| e.to_string())?;

    // Check for running sessions
    let running_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sessions WHERE repo_id = ?1 AND status = 'Running'",
            [repo_id],
            |row| row.get(0),
        )
        .map_err(|e| e.to_string())?;

    if running_count > 0 {
        return Err("Cannot remove repo with running sessions. Stop them first.".to_string());
    }

    conn.execute("DELETE FROM repos WHERE id = ?1", [repo_id])
        .map_err(|e| e.to_string())?;

    Ok(())
}

#[tauri::command]
pub async fn create_session(
    db: tauri::State<'_, Mutex<Connection>>,
    repo_id: i64,
    use_worktree: bool,
    branch: Option<String>,
) -> Result<Session, String> {
    let (repo_path, repo_name) = {
        let conn = db.lock().map_err(|e| e.to_string())?;
        let row: (String, String) = conn
            .query_row(
                "SELECT path, name FROM repos WHERE id = ?1",
                [repo_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .map_err(|e| format!("Repo not found: {e}"))?;
        row
    };

    let (working_dir, worktree_path, branch_name) = if use_worktree {
        let branch = branch.ok_or("Branch name required for worktree")?;
        let home = std::env::var_os("HOME")
            .map(std::path::PathBuf::from)
            .ok_or("Could not find home directory")?;
        let wt_dir = home
            .join("racc-worktrees")
            .join(&repo_name)
            .join(&branch);

        let wt_path = wt_dir.to_string_lossy().to_string();

        if !wt_dir.exists() {
            std::fs::create_dir_all(wt_dir.parent().unwrap())
                .map_err(|e| format!("Failed to create worktree dir: {e}"))?;

            let output = Command::new("git")
                .args(["worktree", "add", &wt_path, "-b", &branch])
                .current_dir(&repo_path)
                .output()
                .map_err(|e| format!("git worktree add failed: {e}"))?;

            if !output.status.success() {
                // Try without -b (branch might already exist)
                let output2 = Command::new("git")
                    .args(["worktree", "add", &wt_path, &branch])
                    .current_dir(&repo_path)
                    .output()
                    .map_err(|e| format!("git worktree add failed: {e}"))?;

                if !output2.status.success() {
                    return Err(format!(
                        "git worktree add failed: {}",
                        String::from_utf8_lossy(&output2.stderr)
                    ));
                }
            }
        }

        (wt_path.clone(), Some(wt_path), branch)
    } else {
        let branch = get_current_branch(&repo_path)?;
        (repo_path.clone(), None, branch)
    };

    let tmux_name = format!("racc::{}::{}", repo_name, branch_name);

    // Check if tmux session already exists
    if tmux_session_exists(&tmux_name) {
        return Err(format!("Session '{}' already exists", tmux_name));
    }

    // Create tmux session
    let output = Command::new("tmux")
        .args([
            "new-session",
            "-d",
            "-s",
            &tmux_name,
            "-x",
            "200",
            "-y",
            "50",
            "-c",
            &working_dir,
        ])
        .output()
        .map_err(|e| format!("Failed to create tmux session: {e}"))?;

    if !output.status.success() {
        return Err(format!(
            "tmux new-session failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    // Start claude in the session
    let _ = Command::new("tmux")
        .args(["send-keys", "-t", &tmux_name, "claude", "Enter"])
        .output();

    // Insert into SQLite
    let conn = db.lock().map_err(|e| e.to_string())?;
    conn.execute(
        "INSERT INTO sessions (repo_id, tmux_session_name, agent, worktree_path, branch, status)
         VALUES (?1, ?2, 'claude-code', ?3, ?4, 'Running')",
        rusqlite::params![repo_id, tmux_name, worktree_path, branch_name],
    )
    .map_err(|e| e.to_string())?;

    let id = conn.last_insert_rowid();
    let (created_at, updated_at): (String, String) = conn
        .query_row(
            "SELECT created_at, updated_at FROM sessions WHERE id = ?1",
            [id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .map_err(|e| e.to_string())?;

    Ok(Session {
        id,
        repo_id,
        tmux_session_name: tmux_name,
        agent: "claude-code".to_string(),
        worktree_path,
        branch: Some(branch_name),
        status: SessionStatus::Running,
        created_at,
        updated_at,
    })
}

#[tauri::command]
pub async fn stop_session(
    db: tauri::State<'_, Mutex<Connection>>,
    session_id: i64,
) -> Result<(), String> {
    let tmux_name = {
        let conn = db.lock().map_err(|e| e.to_string())?;
        let name: String = conn
            .query_row(
                "SELECT tmux_session_name FROM sessions WHERE id = ?1",
                [session_id],
                |row| row.get(0),
            )
            .map_err(|e| format!("Session not found: {e}"))?;
        name
    };

    // Kill tmux session (ignore errors — might already be dead)
    let _ = Command::new("tmux")
        .args(["kill-session", "-t", &tmux_name])
        .output();

    // Update SQLite
    let conn = db.lock().map_err(|e| e.to_string())?;
    conn.execute(
        "UPDATE sessions SET status = 'Completed', updated_at = datetime('now') WHERE id = ?1",
        [session_id],
    )
    .map_err(|e| e.to_string())?;

    Ok(())
}

#[tauri::command]
pub async fn remove_session(
    db: tauri::State<'_, Mutex<Connection>>,
    session_id: i64,
) -> Result<(), String> {
    let conn = db.lock().map_err(|e| e.to_string())?;

    let status: String = conn
        .query_row(
            "SELECT status FROM sessions WHERE id = ?1",
            [session_id],
            |row| row.get(0),
        )
        .map_err(|e| format!("Session not found: {e}"))?;

    if status == "Running" {
        return Err("Cannot remove a running session. Stop it first.".to_string());
    }

    conn.execute("DELETE FROM sessions WHERE id = ?1", [session_id])
        .map_err(|e| e.to_string())?;

    Ok(())
}

#[tauri::command]
pub async fn reconcile_sessions(
    db: tauri::State<'_, Mutex<Connection>>,
) -> Result<Vec<RepoWithSessions>, String> {
    let conn = db.lock().map_err(|e| e.to_string())?;

    // Find all Running sessions
    let mut stmt = conn
        .prepare("SELECT id, tmux_session_name FROM sessions WHERE status = 'Running'")
        .map_err(|e| e.to_string())?;

    let running: Vec<(i64, String)> = stmt
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
        .map_err(|e| e.to_string())?
        .filter_map(|r| r.ok())
        .collect();

    // Check each against tmux
    for (id, name) in &running {
        if !tmux_session_exists(name) {
            conn.execute(
                "UPDATE sessions SET status = 'Disconnected', updated_at = datetime('now') WHERE id = ?1",
                [id],
            )
            .map_err(|e| e.to_string())?;
        }
    }

    query_repos_with_sessions(&conn)
}
```

- [ ] **Step 2: Verify cargo check passes**

Run: `cd src-tauri && cargo check`

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/commands/session.rs
git commit -m "feat(session): rewrite session module with SQLite-backed CRUD"
```

---

### Task 4: Wire up lib.rs — DB state, dialog plugin, command registration

**Files:**
- Modify: `src-tauri/src/lib.rs`

- [ ] **Step 1: Replace the entire `src-tauri/src/lib.rs`**

```rust
mod commands;

use std::sync::Mutex;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let db = commands::db::init_db().expect("Failed to initialize database");

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(Mutex::new(db))
        .invoke_handler(tauri::generate_handler![
            commands::session::import_repo,
            commands::session::list_repos,
            commands::session::remove_repo,
            commands::session::create_session,
            commands::session::stop_session,
            commands::session::remove_session,
            commands::session::reconcile_sessions,
            commands::tmux::send_keys,
            commands::tmux::send_special_key,
            commands::tmux::capture_pane,
            commands::tmux::resize_pane,
            commands::git::create_worktree,
            commands::git::delete_worktree,
            commands::git::get_diff,
            commands::cost::get_project_costs,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
```

- [ ] **Step 2: Verify cargo build succeeds**

Run: `cd src-tauri && cargo build`
Expected: compiles without errors

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/lib.rs
git commit -m "feat(app): wire DB state, dialog plugin, and new session commands"
```

---

## Chunk 2: Frontend — Types, Store, Components

### Task 5: Rewrite TypeScript types

**Files:**
- Rewrite: `src/types/session.ts`

- [ ] **Step 1: Replace the entire `src/types/session.ts`**

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
  updated_at: string;
}

export interface RepoWithSessions {
  repo: Repo;
  sessions: Session[];
}
```

- [ ] **Step 2: Commit**

```bash
git add src/types/session.ts
git commit -m "feat(types): rewrite session types for repo-centric model"
```

---

### Task 6: Rewrite Zustand store

**Files:**
- Rewrite: `src/stores/sessionStore.ts`

- [ ] **Step 1: Replace the entire `src/stores/sessionStore.ts`**

```typescript
import { create } from "zustand";
import { invoke } from "@tauri-apps/api/core";
import type { Repo, Session, RepoWithSessions } from "../types/session";

interface SessionState {
  repos: RepoWithSessions[];
  activeSessionId: number | null;
  loading: boolean;
  error: string | null;

  getActiveSession: () => { session: Session; repo: Repo } | null;

  initialize: () => Promise<void>;
  importRepo: (path: string) => Promise<void>;
  removeRepo: (repoId: number) => Promise<void>;
  createSession: (
    repoId: number,
    useWorktree: boolean,
    branch?: string,
  ) => Promise<void>;
  stopSession: (sessionId: number) => Promise<void>;
  removeSession: (sessionId: number) => Promise<void>;
  setActiveSession: (id: number) => void;
  clearError: () => void;
}

export const useSessionStore = create<SessionState>((set, get) => ({
  repos: [],
  activeSessionId: null,
  loading: false,
  error: null,

  getActiveSession: () => {
    const { repos, activeSessionId } = get();
    if (activeSessionId === null) return null;
    for (const rws of repos) {
      const session = rws.sessions.find((s) => s.id === activeSessionId);
      if (session) return { session, repo: rws.repo };
    }
    return null;
  },

  initialize: async () => {
    set({ loading: true, error: null });
    try {
      const repos = await invoke<RepoWithSessions[]>("reconcile_sessions");
      set({ repos, loading: false });
    } catch (e) {
      set({ repos: [], loading: false, error: String(e) });
    }
  },

  importRepo: async (path) => {
    set({ error: null });
    try {
      await invoke<Repo>("import_repo", { path });
      const repos = await invoke<RepoWithSessions[]>("list_repos");
      set({ repos });
    } catch (e) {
      set({ error: String(e) });
      throw e;
    }
  },

  removeRepo: async (repoId) => {
    set({ error: null });
    try {
      await invoke("remove_repo", { repoId });
      const repos = await invoke<RepoWithSessions[]>("list_repos");
      // Clear active session if it belonged to removed repo
      const { activeSessionId } = get();
      if (activeSessionId !== null) {
        const stillExists = repos.some((r) =>
          r.sessions.some((s) => s.id === activeSessionId),
        );
        set({
          repos,
          activeSessionId: stillExists ? activeSessionId : null,
        });
      } else {
        set({ repos });
      }
    } catch (e) {
      set({ error: String(e) });
    }
  },

  createSession: async (repoId, useWorktree, branch) => {
    set({ error: null });
    try {
      const session = await invoke<Session>("create_session", {
        repoId,
        useWorktree,
        branch: branch || null,
      });
      const repos = await invoke<RepoWithSessions[]>("list_repos");
      set({ repos, activeSessionId: session.id });
    } catch (e) {
      set({ error: String(e) });
      throw e;
    }
  },

  stopSession: async (sessionId) => {
    try {
      await invoke("stop_session", { sessionId });
      const repos = await invoke<RepoWithSessions[]>("list_repos");
      const { activeSessionId } = get();
      set({
        repos,
        activeSessionId:
          activeSessionId === sessionId ? null : activeSessionId,
      });
    } catch (e) {
      set({ error: String(e) });
    }
  },

  removeSession: async (sessionId) => {
    try {
      await invoke("remove_session", { sessionId });
      const repos = await invoke<RepoWithSessions[]>("list_repos");
      const { activeSessionId } = get();
      set({
        repos,
        activeSessionId:
          activeSessionId === sessionId ? null : activeSessionId,
      });
    } catch (e) {
      set({ error: String(e) });
    }
  },

  setActiveSession: (id) => set({ activeSessionId: id }),

  clearError: () => set({ error: null }),
}));
```

- [ ] **Step 2: Commit**

```bash
git add src/stores/sessionStore.ts
git commit -m "feat(store): rewrite Zustand store for repo-centric session management"
```

---

### Task 7: Create ImportRepoDialog component

**Files:**
- Create: `src/components/Sidebar/ImportRepoDialog.tsx`

- [ ] **Step 1: Create `src/components/Sidebar/ImportRepoDialog.tsx`**

```tsx
import { useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { useSessionStore } from "../../stores/sessionStore";

export function ImportRepoDialog() {
  const [error, setError] = useState<string | null>(null);
  const [importing, setImporting] = useState(false);
  const importRepo = useSessionStore((s) => s.importRepo);

  const handleImport = async () => {
    setError(null);
    setImporting(true);
    try {
      const selected = await open({ directory: true, multiple: false });
      if (!selected) {
        setImporting(false);
        return; // User cancelled
      }
      await importRepo(selected);
    } catch (e) {
      setError(String(e));
    } finally {
      setImporting(false);
    }
  };

  return (
    <div>
      <button
        onClick={handleImport}
        disabled={importing}
        className="flex w-full items-center gap-2 rounded px-3 py-2 text-xs text-zinc-400 hover:bg-surface-2 hover:text-zinc-200 disabled:opacity-50"
      >
        <span className="text-base leading-none">+</span>
        {importing ? "Importing..." : "Import Repo"}
      </button>
      {error && (
        <p className="mx-3 mt-1 text-xs text-red-400">{error}</p>
      )}
    </div>
  );
}
```

- [ ] **Step 2: Commit**

```bash
git add src/components/Sidebar/ImportRepoDialog.tsx
git commit -m "feat(ui): add ImportRepoDialog with native folder picker"
```

---

### Task 8: Create NewAgentDialog component

**Files:**
- Create: `src/components/Sidebar/NewAgentDialog.tsx`

- [ ] **Step 1: Create `src/components/Sidebar/NewAgentDialog.tsx`**

```tsx
import { useState } from "react";
import { useSessionStore } from "../../stores/sessionStore";

interface Props {
  repoId: number;
  open: boolean;
  onClose: () => void;
}

export function NewAgentDialog({ repoId, open: isOpen, onClose }: Props) {
  const [useWorktree, setUseWorktree] = useState(false);
  const [branch, setBranch] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [creating, setCreating] = useState(false);
  const createSession = useSessionStore((s) => s.createSession);

  if (!isOpen) return null;

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (useWorktree && !branch.trim()) return;

    setCreating(true);
    setError(null);
    try {
      await createSession(repoId, useWorktree, useWorktree ? branch.trim() : undefined);
      setBranch("");
      setUseWorktree(false);
      onClose();
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setCreating(false);
    }
  };

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === "Escape") onClose();
  };

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/60"
      onKeyDown={handleKeyDown}
    >
      <form
        onSubmit={handleSubmit}
        className="w-80 rounded-lg border border-surface-3 bg-surface-1 p-5 shadow-2xl"
      >
        <h2 className="mb-4 text-sm font-semibold text-zinc-200">
          Launch Agent
        </h2>

        <fieldset className="mb-4 space-y-2">
          <label className="flex items-center gap-2 text-xs text-zinc-300">
            <input
              type="radio"
              name="mode"
              checked={!useWorktree}
              onChange={() => setUseWorktree(false)}
              className="accent-accent"
            />
            Run in repo
          </label>
          <label className="flex items-center gap-2 text-xs text-zinc-300">
            <input
              type="radio"
              name="mode"
              checked={useWorktree}
              onChange={() => setUseWorktree(true)}
              className="accent-accent"
            />
            Create worktree
          </label>
        </fieldset>

        {useWorktree && (
          <label className="mb-4 block">
            <span className="mb-1 block text-xs text-zinc-400">
              Branch name
            </span>
            <input
              type="text"
              value={branch}
              onChange={(e) => setBranch(e.target.value)}
              placeholder="feat/my-feature"
              autoFocus
              className="w-full rounded border border-surface-3 bg-surface-2 px-3 py-1.5 text-sm text-white placeholder-zinc-600 outline-none focus:border-accent"
            />
          </label>
        )}

        {error && (
          <p className="mb-3 rounded bg-red-500/10 px-3 py-2 text-xs text-red-400">
            {error}
          </p>
        )}

        <div className="flex justify-end gap-2">
          <button
            type="button"
            onClick={onClose}
            className="rounded px-3 py-1.5 text-xs text-zinc-400 hover:text-zinc-200"
          >
            Cancel
          </button>
          <button
            type="submit"
            disabled={creating || (useWorktree && !branch.trim())}
            className="rounded bg-accent px-3 py-1.5 text-xs font-medium text-white hover:bg-accent-hover disabled:opacity-50"
          >
            {creating ? "Launching..." : "Launch"}
          </button>
        </div>
      </form>
    </div>
  );
}
```

- [ ] **Step 2: Commit**

```bash
git add src/components/Sidebar/NewAgentDialog.tsx
git commit -m "feat(ui): add NewAgentDialog with worktree choice"
```

---

### Task 9: Rewrite Sidebar with repo-grouped layout

**Files:**
- Rewrite: `src/components/Sidebar/Sidebar.tsx`
- Delete: `src/components/Sidebar/NewSessionDialog.tsx`

- [ ] **Step 1: Replace the entire `src/components/Sidebar/Sidebar.tsx`**

```tsx
import { useState } from "react";
import { useSessionStore } from "../../stores/sessionStore";
import { ImportRepoDialog } from "./ImportRepoDialog";
import { NewAgentDialog } from "./NewAgentDialog";
import type { SessionStatus } from "../../types/session";

const statusColor: Record<SessionStatus, string> = {
  Running: "bg-status-running",
  Completed: "bg-status-completed",
  Disconnected: "bg-status-disconnected",
  Error: "bg-status-error",
};

export function Sidebar() {
  const repos = useSessionStore((s) => s.repos);
  const activeSessionId = useSessionStore((s) => s.activeSessionId);
  const setActiveSession = useSessionStore((s) => s.setActiveSession);
  const stopSession = useSessionStore((s) => s.stopSession);
  const removeSession = useSessionStore((s) => s.removeSession);
  const removeRepo = useSessionStore((s) => s.removeRepo);

  const [expandedRepos, setExpandedRepos] = useState<Set<number>>(new Set());
  const [agentDialogRepoId, setAgentDialogRepoId] = useState<number | null>(null);

  const toggleRepo = (repoId: number) => {
    setExpandedRepos((prev) => {
      const next = new Set(prev);
      if (next.has(repoId)) next.delete(repoId);
      else next.add(repoId);
      return next;
    });
  };

  // Auto-expand repos that have sessions
  const isExpanded = (repoId: number) => {
    const rws = repos.find((r) => r.repo.id === repoId);
    return expandedRepos.has(repoId) || (rws?.sessions.length ?? 0) > 0;
  };

  return (
    <aside className="flex w-56 flex-col overflow-y-auto border-r border-surface-3 bg-surface-1">
      <div className="border-b border-surface-3 px-3 py-2">
        <h1 className="text-xs font-bold uppercase tracking-widest text-zinc-500">
          Racc
        </h1>
      </div>

      <ImportRepoDialog />

      <div className="flex-1 overflow-y-auto px-1 py-1">
        {repos.length === 0 && (
          <p className="px-3 py-4 text-center text-xs text-zinc-600">
            No repos imported yet
          </p>
        )}

        {repos.map(({ repo, sessions }) => (
          <div key={repo.id} className="mb-1">
            {/* Repo row */}
            <div className="group flex items-center rounded px-2 py-1.5 hover:bg-surface-2">
              <button
                onClick={() => toggleRepo(repo.id)}
                className="mr-1 text-xs text-zinc-500"
              >
                {isExpanded(repo.id) ? "▼" : "▶"}
              </button>
              <span
                className="flex-1 truncate text-xs font-medium text-zinc-300 cursor-pointer"
                onClick={() => toggleRepo(repo.id)}
                title={repo.path}
              >
                {repo.name}
              </span>
              <button
                onClick={() => setAgentDialogRepoId(repo.id)}
                className="ml-1 hidden rounded px-1 text-xs text-zinc-500 hover:text-accent group-hover:block"
                title="Launch agent"
              >
                +
              </button>
              <button
                onClick={() => removeRepo(repo.id)}
                className="ml-1 hidden rounded px-1 text-xs text-zinc-500 hover:text-red-400 group-hover:block"
                title="Remove repo"
              >
                ×
              </button>
            </div>

            {/* Sessions (nested) */}
            {isExpanded(repo.id) &&
              sessions.map((session) => (
                <div
                  key={session.id}
                  onClick={() => {
                    if (session.status === "Running") {
                      setActiveSession(session.id);
                    }
                  }}
                  className={`group ml-4 flex cursor-pointer items-center gap-2 rounded px-2 py-1 ${
                    session.id === activeSessionId
                      ? "bg-surface-3"
                      : "hover:bg-surface-2"
                  }`}
                >
                  <span
                    className={`h-1.5 w-1.5 rounded-full ${statusColor[session.status]}`}
                  />
                  <span className="flex-1 truncate text-xs text-zinc-400">
                    {session.branch ?? "main"}
                  </span>
                  {session.status === "Running" ? (
                    <button
                      onClick={(e) => {
                        e.stopPropagation();
                        stopSession(session.id);
                      }}
                      className="hidden text-xs text-zinc-500 hover:text-red-400 group-hover:block"
                      title="Stop session"
                    >
                      ■
                    </button>
                  ) : (
                    <button
                      onClick={(e) => {
                        e.stopPropagation();
                        removeSession(session.id);
                      }}
                      className="hidden text-xs text-zinc-500 hover:text-red-400 group-hover:block"
                      title="Remove session"
                    >
                      ×
                    </button>
                  )}
                </div>
              ))}
          </div>
        ))}
      </div>

      {agentDialogRepoId !== null && (
        <NewAgentDialog
          repoId={agentDialogRepoId}
          open={true}
          onClose={() => setAgentDialogRepoId(null)}
        />
      )}
    </aside>
  );
}
```

- [ ] **Step 2: Delete `src/components/Sidebar/NewSessionDialog.tsx`**

Run: `rm src/components/Sidebar/NewSessionDialog.tsx`

- [ ] **Step 3: Commit**

```bash
git add src/components/Sidebar/Sidebar.tsx
git rm src/components/Sidebar/NewSessionDialog.tsx
git commit -m "feat(ui): rewrite sidebar with repo-grouped layout"
```

---

## Chunk 3: Integration — Terminal, CostTracker, StatusBar, App

### Task 10: Update Terminal.tsx to use getActiveSession()

**Files:**
- Modify: `src/components/Terminal/Terminal.tsx:9`

- [ ] **Step 1: Update Terminal.tsx**

In `src/components/Terminal/Terminal.tsx`, change line 9 from:

```typescript
const activeSessionId = useSessionStore((s) => s.activeSessionId);
```

To:

```typescript
const activeSession = useSessionStore((s) => s.getActiveSession());
const sessionId = activeSession?.session.tmux_session_name ?? null;
```

Then update any references to `activeSessionId` in the component:
- Replace `activeSessionId` with `sessionId` where it's passed to `useTmuxBridge` or used as a dependency
- Replace `!activeSessionId` with `!sessionId` in the fallback UI render condition

- [ ] **Step 2: Commit**

```bash
git add src/components/Terminal/Terminal.tsx
git commit -m "feat(terminal): use getActiveSession() for tmux bridge"
```

---

### Task 11: Update CostTracker to use getActiveSession()

**Files:**
- Modify: `src/components/CostTracker/CostTracker.tsx`

- [ ] **Step 1: Update CostTracker.tsx**

Replace the current session lookup logic (lines that read `activeSessionId`, `sessions`, `activeSession`, `worktreePath`) with:

```typescript
const active = useSessionStore((s) => s.getActiveSession());
const worktreePath = active?.session.worktree_path ?? active?.repo.path;
```

Remove the separate `activeSessionId` and `sessions` selectors — `getActiveSession()` replaces both.

The `useEffect` dependency should change from `[worktreePath]` — it stays the same since `worktreePath` is still the derived value.

- [ ] **Step 2: Commit**

```bash
git add src/components/CostTracker/CostTracker.tsx
git commit -m "feat(cost): use getActiveSession() for cost path resolution"
```

---

### Task 12: Update StatusBar + cleanup Tailwind

**Files:**
- Modify: `src/components/Dashboard/StatusBar.tsx`
- Modify: `tailwind.config.ts`

- [ ] **Step 1: Update StatusBar.tsx**

The current StatusBar counts sessions with status `"Running"` or `"Waiting"`. Update to work with the new nested structure:

```typescript
const repos = useSessionStore((s) => s.repos);
const activeSessions = repos.flatMap((r) =>
  r.sessions.filter((s) => s.status === "Running"),
).length;
```

Replace the old `sessions` selector and the `"Waiting"` status filter.

- [ ] **Step 2: Clean up tailwind.config.ts**

Remove `waiting` and `paused` from the `status` colors (lines ~20-21):

```typescript
status: {
  running: "#22c55e",
  error: "#ef4444",
  disconnected: "#f97316",
  completed: "#3b82f6",
}
```

- [ ] **Step 3: Commit**

```bash
git add src/components/Dashboard/StatusBar.tsx tailwind.config.ts
git commit -m "feat(ui): update StatusBar for new model, clean up Tailwind status colors"
```

---

### Task 13: Update App.tsx — use initialize(), remove polling

**Files:**
- Modify: `src/App.tsx`

- [ ] **Step 1: Update App.tsx**

Replace the current `useEffect` that calls `fetchSessions` with polling:

```typescript
// Old:
const fetchSessions = useSessionStore((s) => s.fetchSessions);
useEffect(() => {
  fetchSessions();
  const interval = setInterval(fetchSessions, 5000);
  return () => clearInterval(interval);
}, [fetchSessions]);

// New:
const initialize = useSessionStore((s) => s.initialize);
useEffect(() => {
  initialize();
}, [initialize]);
```

- [ ] **Step 2: Verify frontend builds**

Run: `bun run build`
Expected: no errors

- [ ] **Step 3: Commit**

```bash
git add src/App.tsx
git commit -m "feat(app): use initialize() with reconciliation on mount"
```

---

## Chunk 4: Verification

### Task 14: Full build and manual test

- [ ] **Step 1: Run full Rust build**

Run: `cd src-tauri && cargo build`
Expected: compiles without errors

- [ ] **Step 2: Run full frontend build**

Run: `bun run build`
Expected: no errors

- [ ] **Step 3: Manual test with `bun tauri dev`**

Run: `bun tauri dev`

Verify:
1. App launches, sidebar shows "Import Repo" button, no repos listed
2. Click "Import Repo" → native Finder dialog opens
3. Select a git repo → appears in sidebar
4. Select a non-git folder → shows error
5. Click [+] on repo → Launch Agent dialog appears with radio choice
6. Launch with "Run in repo" → tmux session created, terminal connects
7. Launch with "Create worktree" → worktree created at ~/racc-worktrees, terminal connects
8. Stop session → status changes to Completed
9. Remove session → disappears from sidebar
10. Restart app → repos and session history persist, dead sessions marked Disconnected
11. Cost tracker shows data for active session
