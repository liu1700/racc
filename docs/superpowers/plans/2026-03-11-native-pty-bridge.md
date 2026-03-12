# Native PTY Terminal Bridge Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace tmux-based terminal polling with native PTY streaming via `tauri-plugin-pty`, eliminating the tmux dependency and achieving real-time terminal output.

**Architecture:** Frontend-managed PTY lifecycle using the `tauri-pty` JS package (the official Tauri PTY plugin). A `ptyManager.ts` singleton manages PTY instances and output buffers per session. The Rust backend is simplified to only handle persistence (DB + git worktrees) — all terminal I/O moves to the frontend via the plugin. DB migration v2 drops the `tmux_session_name` column.

**Tech Stack:** tauri-plugin-pty (Rust plugin + `tauri-pty` JS), xterm.js, Zustand, SQLite

**Design note:** The approved design specified `portable-pty` with custom Rust PTY commands. This plan uses `tauri-plugin-pty` instead — a purpose-built Tauri plugin that provides the same architecture (no tmux, no polling, real-time push) with significantly less custom code. The PTY spawn/write/resize/kill API is exposed directly to the frontend via the plugin's JS bindings, eliminating the need for a custom `pty.rs` module.

---

## File Structure

### Files to Create
| File | Responsibility |
|------|---------------|
| `src/services/ptyManager.ts` | Singleton managing PTY instances per session. Spawns PTY, tracks output buffers, exposes write/resize/kill. |
| `src/hooks/usePtyBridge.ts` | React hook connecting active session's PTY to xterm.js (replaces `useTmuxBridge.ts`). |

### Files to Modify
| File | Changes |
|------|---------|
| `src-tauri/Cargo.toml` | Add `tauri-plugin-pty` dependency |
| `package.json` | Add `tauri-pty` JS dependency |
| `src-tauri/src/lib.rs` | Register PTY plugin, remove tmux command handlers |
| `src-tauri/src/commands/mod.rs` | Remove `pub mod tmux;` |
| `src-tauri/src/commands/db.rs` | Add migration v2: recreate sessions table without `tmux_session_name` |
| `src-tauri/src/commands/session.rs` | Remove tmux logic from create/stop/reconcile, remove `tmux_session_name` from Session struct |
| `src/types/session.ts` | Remove `tmux_session_name` from Session interface |
| `src/stores/sessionStore.ts` | Call ptyManager.spawn on create, ptyManager.kill on stop |
| `src/components/Terminal/Terminal.tsx` | Swap `useTmuxBridge` for `usePtyBridge`, use `session.id` |
| `src-tauri/capabilities/default.json` | Add PTY plugin permissions |
| `CLAUDE.md` | Update architecture description (tmux → PTY) |
| `wiki/*.md` | Update any tmux references in design docs |

### Files to Delete
| File | Reason |
|------|--------|
| `src-tauri/src/commands/tmux.rs` | Entire tmux wrapper replaced by PTY plugin |
| `src/hooks/useTmuxBridge.ts` | Replaced by `usePtyBridge.ts` |

---

## Chunk 1: Backend — Dependencies, DB Migration, Session Refactor

### Task 0: Verify tauri-plugin-pty API

**Files:** None (research only)

- [ ] **Step 1: Install and inspect the tauri-pty package types**

```bash
cd /Users/yuchenliu/Documents/otte && bun add tauri-pty
```

Then inspect the actual TypeScript types:

```bash
cat node_modules/tauri-pty/dist/index.d.ts
```

Verify these exports exist and match the plan's assumptions:
- `spawn(shell, args, opts)` → returns `PtyProcess`
- `PtyProcess.onData(cb: (data: Uint8Array) => void)` → disposable
- `PtyProcess.onExit(cb: (e: { exitCode: number }) => void)` → disposable
- `PtyProcess.write(data: string)` → void
- `PtyProcess.resize(cols, rows)` → void
- `PtyProcess.kill()` → void
- `PtyProcess.pid` → number

If the API differs, update all downstream code in Tasks 6-9 before proceeding.

- [ ] **Step 2: Revert the bun add if API is fundamentally incompatible**

If the plugin doesn't provide the needed functionality, fall back to the `portable-pty` approach from the original design and rewrite Tasks 6-9.

---

### Task 1: Add tauri-plugin-pty dependencies

**Files:**
- Modify: `src-tauri/Cargo.toml`
- Modify: `package.json`

- [ ] **Step 1: Add Rust dependency**

In `src-tauri/Cargo.toml`, add to `[dependencies]`:

```toml
tauri-plugin-pty = "2"
```

- [ ] **Step 2: Add JS dependency**

```bash
cd /Users/yuchenliu/Documents/otte && bun add tauri-pty
```

- [ ] **Step 3: Register plugin in lib.rs**

In `src-tauri/src/lib.rs`, add the plugin registration:

```rust
mod commands;

use std::sync::Mutex;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let db = commands::db::init_db().expect("Failed to initialize database");

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_pty::init())
        .manage(Mutex::new(db))
        .invoke_handler(tauri::generate_handler![
            commands::session::import_repo,
            commands::session::list_repos,
            commands::session::remove_repo,
            commands::session::create_session,
            commands::session::stop_session,
            commands::session::remove_session,
            commands::session::reconcile_sessions,
            commands::git::create_worktree,
            commands::git::delete_worktree,
            commands::git::get_diff,
            commands::cost::get_project_costs,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
```

Note: All 4 tmux command handlers are removed. No new PTY handlers needed (the plugin registers its own).

- [ ] **Step 4: Add PTY plugin permissions**

In `src-tauri/capabilities/default.json`, add `"pty:default"` to permissions:

```json
{
  "$schema": "../gen/schemas/desktop-schema.json",
  "identifier": "default",
  "description": "Default permissions for Racc",
  "windows": ["main"],
  "permissions": [
    "core:default",
    "shell:allow-open",
    "shell:allow-execute",
    "dialog:default",
    "dialog:allow-open",
    "pty:default"
  ]
}
```

- [ ] **Step 5: Verify Rust compiles**

```bash
cd /Users/yuchenliu/Documents/otte/src-tauri && cargo check
```

Expected: Compiles (tmux.rs still exists but handlers removed from lib.rs — the module is still declared but unused commands are OK for now).

If `cargo check` fails because of the missing tmux handlers, do Task 2 (remove tmux module) first.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/Cargo.toml src-tauri/Cargo.lock src-tauri/src/lib.rs src-tauri/capabilities/default.json package.json bun.lockb
git commit -m "feat: add tauri-plugin-pty, remove tmux command handlers"
```

---

### Task 2: Remove tmux module

**Files:**
- Delete: `src-tauri/src/commands/tmux.rs`
- Modify: `src-tauri/src/commands/mod.rs`

- [ ] **Step 1: Delete tmux.rs**

```bash
rm /Users/yuchenliu/Documents/otte/src-tauri/src/commands/tmux.rs
```

- [ ] **Step 2: Remove from mod.rs**

Update `src-tauri/src/commands/mod.rs`:

```rust
pub mod cost;
pub mod db;
pub mod git;
pub mod session;
```

- [ ] **Step 3: Verify Rust compiles**

```bash
cd /Users/yuchenliu/Documents/otte/src-tauri && cargo check
```

Expected: Compiles successfully. The `tmux_session_exists` helper in `session.rs` and tmux references still exist but will be cleaned up in Task 4.

- [ ] **Step 4: Commit**

```bash
git add -A src-tauri/src/commands/tmux.rs src-tauri/src/commands/mod.rs
git commit -m "refactor: remove tmux command module"
```

---

### Task 3: DB migration v2 — drop tmux_session_name

**Files:**
- Modify: `src-tauri/src/commands/db.rs`

- [ ] **Step 1: Add migration v2**

Add after the `if version < 1` block in `init_db()`:

```rust
if version < 2 {
    conn.execute_batch(
        "
        BEGIN;

        CREATE TABLE sessions_new (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            repo_id INTEGER NOT NULL REFERENCES repos(id) ON DELETE CASCADE,
            agent TEXT NOT NULL DEFAULT 'claude-code',
            worktree_path TEXT,
            branch TEXT,
            status TEXT NOT NULL DEFAULT 'Running',
            created_at TEXT NOT NULL DEFAULT (datetime('now')),
            updated_at TEXT NOT NULL DEFAULT (datetime('now'))
        );

        INSERT INTO sessions_new (id, repo_id, agent, worktree_path, branch, status, created_at, updated_at)
            SELECT id, repo_id, agent, worktree_path, branch, status, created_at, updated_at
            FROM sessions;

        DROP TABLE sessions;
        ALTER TABLE sessions_new RENAME TO sessions;

        PRAGMA user_version = 2;

        COMMIT;
        ",
    )
    .map_err(|e| format!("Migration v2 failed: {e}"))?;
}
```

- [ ] **Step 2: Verify Rust compiles**

```bash
cd /Users/yuchenliu/Documents/otte/src-tauri && cargo check
```

Expected: Compiles. Session struct in session.rs still references `tmux_session_name` — that's fixed in Task 4.

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/commands/db.rs
git commit -m "feat: add DB migration v2 dropping tmux_session_name column"
```

---

### Task 4: Refactor session.rs — remove all tmux logic

**Files:**
- Modify: `src-tauri/src/commands/session.rs`

- [ ] **Step 1: Update Session struct and helpers**

Replace the entire file with:

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
            "SELECT id, repo_id, agent, worktree_path, branch, status, created_at, updated_at
             FROM sessions WHERE repo_id = ? ORDER BY created_at DESC",
        )
        .map_err(|e| e.to_string())?;

    let mut result = Vec::new();
    for repo in repos {
        let sessions: Vec<Session> = session_stmt
            .query_map([repo.id], |row| {
                let status_str: String = row.get(5)?;
                Ok(Session {
                    id: row.get(0)?,
                    repo_id: row.get(1)?,
                    agent: row.get(2)?,
                    worktree_path: row.get(3)?,
                    branch: row.get(4)?,
                    status: SessionStatus::from_str(&status_str),
                    created_at: row.get(6)?,
                    updated_at: row.get(7)?,
                })
            })
            .map_err(|e| e.to_string())?
            .filter_map(|r| r.ok())
            .collect();

        result.push(RepoWithSessions { repo, sessions });
    }

    Ok(result)
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

    let (worktree_path, branch_name) = if use_worktree {
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

        (Some(wt_path), branch)
    } else {
        let branch = get_current_branch(&repo_path)?;
        (None, branch)
    };

    let conn = db.lock().map_err(|e| e.to_string())?;
    conn.execute(
        "INSERT INTO sessions (repo_id, agent, worktree_path, branch, status)
         VALUES (?1, 'claude-code', ?2, ?3, 'Running')",
        rusqlite::params![repo_id, worktree_path, branch_name],
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

    // With native PTY, there's no external process to check.
    // On app startup, all previously "Running" sessions are stale
    // because PTY state is in-memory and lost on restart.
    conn.execute(
        "UPDATE sessions SET status = 'Disconnected', updated_at = datetime('now') WHERE status = 'Running'",
        [],
    )
    .map_err(|e| e.to_string())?;

    query_repos_with_sessions(&conn)
}
```

Key changes from original:
- `Session` struct: removed `tmux_session_name` field
- `create_session`: removed tmux new-session + send-keys "claude". Returns session; frontend spawns PTY.
- `stop_session`: removed tmux kill-session. Just updates DB. Frontend kills PTY.
- `reconcile_sessions`: marks ALL Running sessions as Disconnected (no tmux check).
- Removed `tmux_session_exists` helper.
- Removed `working_dir` local (worktree_path or repo_path is resolved by frontend).

- [ ] **Step 2: Verify Rust compiles**

```bash
cd /Users/yuchenliu/Documents/otte/src-tauri && cargo check
```

Expected: Compiles successfully with no warnings about tmux.

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/commands/session.rs
git commit -m "refactor: remove tmux logic from session management, simplify to DB-only"
```

---

## Chunk 2: Frontend — PTY Manager, Bridge Hook, Component Updates

### Task 5: Update frontend Session type

**Files:**
- Modify: `src/types/session.ts`

- [ ] **Step 1: Remove tmux_session_name from Session interface**

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
git commit -m "refactor: remove tmux_session_name from Session type"
```

---

### Task 6: Create ptyManager.ts

**Files:**
- Create: `src/services/ptyManager.ts`

This singleton manages PTY instances and output buffers. It decouples PTY lifecycle from React component lifecycle so PTYs survive session switching.

- [ ] **Step 1: Write ptyManager.ts**

```typescript
import { spawn, type PtyProcess } from "tauri-pty";

const MAX_BUFFER_SIZE = 1024 * 1024; // 1MB output buffer per session

// Default shell — process.env is not available in WebView context.
// macOS default; Windows users would need "powershell.exe".
const DEFAULT_SHELL = "/bin/zsh";

interface PtyEntry {
  pty: PtyProcess;
  buffer: Uint8Array[];
  bufferSize: number;
  listeners: Set<(data: Uint8Array) => void>;
  exited: boolean;
  exitCode: number | null;
}

const entries = new Map<number, PtyEntry>();

export function spawnPty(
  sessionId: number,
  cwd: string,
  cols: number,
  rows: number,
  agentCmd?: string,
): void {
  if (entries.has(sessionId)) return;

  const pty = spawn(DEFAULT_SHELL, [], {
    cols,
    rows,
    cwd,
    env: { TERM: "xterm-256color" },
  });

  const entry: PtyEntry = {
    pty,
    buffer: [],
    bufferSize: 0,
    listeners: new Set(),
    exited: false,
    exitCode: null,
  };

  pty.onData((data: Uint8Array) => {
    // Accumulate in buffer
    entry.buffer.push(data);
    entry.bufferSize += data.length;

    // Trim buffer if over max size (drop oldest chunks)
    while (entry.bufferSize > MAX_BUFFER_SIZE && entry.buffer.length > 1) {
      const dropped = entry.buffer.shift()!;
      entry.bufferSize -= dropped.length;
    }

    // Notify active listeners
    for (const listener of entry.listeners) {
      listener(data);
    }
  });

  pty.onExit(({ exitCode }) => {
    entry.exited = true;
    entry.exitCode = exitCode;
    // Notify listeners of exit via a terminal message
    const msg = new TextEncoder().encode(`\r\n[Process exited with code ${exitCode}]\r\n`);
    for (const listener of entry.listeners) {
      listener(msg);
    }
  });

  entries.set(sessionId, entry);

  // Send agent command after a short delay to let shell initialize
  if (agentCmd) {
    setTimeout(() => {
      pty.write(agentCmd + "\n");
    }, 100);
  }
}

export function writePty(sessionId: number, data: string): void {
  entries.get(sessionId)?.pty.write(data);
}

export function resizePty(sessionId: number, cols: number, rows: number): void {
  entries.get(sessionId)?.pty.resize(cols, rows);
}

export function killPty(sessionId: number): void {
  const entry = entries.get(sessionId);
  if (!entry) return;
  if (!entry.exited) {
    entry.pty.kill();
  }
  entry.listeners.clear();
  entries.delete(sessionId);
}

/** Subscribe to live PTY output. Returns unsubscribe function. */
export function subscribe(
  sessionId: number,
  listener: (data: Uint8Array) => void,
): (() => void) | null {
  const entry = entries.get(sessionId);
  if (!entry) return null;
  entry.listeners.add(listener);
  return () => entry.listeners.delete(listener);
}

/** Get accumulated output buffer for replaying into xterm on session switch. */
export function getBuffer(sessionId: number): Uint8Array[] {
  return entries.get(sessionId)?.buffer ?? [];
}

/** Check if a PTY is alive for a given session. */
export function isAlive(sessionId: number): boolean {
  const entry = entries.get(sessionId);
  return entry !== undefined && !entry.exited;
}

/** Kill all PTYs (for app cleanup). */
export function killAll(): void {
  for (const [id] of entries) {
    killPty(id);
  }
}
```

- [ ] **Step 2: Commit**

```bash
git add src/services/ptyManager.ts
git commit -m "feat: add ptyManager service for PTY lifecycle management"
```

---

### Task 7: Create usePtyBridge.ts

**Files:**
- Create: `src/hooks/usePtyBridge.ts`

- [ ] **Step 1: Write usePtyBridge.ts**

```typescript
import { useEffect, useRef } from "react";
import type { Terminal } from "@xterm/xterm";
import { subscribe, getBuffer, writePty, resizePty } from "@/services/ptyManager";

interface UsePtyBridgeOptions {
  sessionId: number | null;
  terminal: Terminal | null;
}

export function usePtyBridge({ sessionId, terminal }: UsePtyBridgeOptions) {
  const prevSessionRef = useRef<number | null>(null);

  // Connect PTY output to xterm
  useEffect(() => {
    if (sessionId === null || !terminal) return;

    // On session switch: clear terminal and replay buffer
    if (sessionId !== prevSessionRef.current) {
      terminal.reset();
      const buffer = getBuffer(sessionId);
      for (const chunk of buffer) {
        terminal.write(chunk);
      }
      prevSessionRef.current = sessionId;
    }

    // Subscribe to live output
    const unsub = subscribe(sessionId, (data) => {
      terminal.write(data);
    });

    return () => {
      unsub?.();
    };
  }, [sessionId, terminal]);

  // Forward keyboard input to PTY
  useEffect(() => {
    if (sessionId === null || !terminal) return;

    const disposable = terminal.onData((data: string) => {
      writePty(sessionId, data);
    });

    return () => disposable.dispose();
  }, [sessionId, terminal]);

  // Sync terminal size to PTY
  useEffect(() => {
    if (sessionId === null || !terminal) return;

    resizePty(sessionId, terminal.cols, terminal.rows);

    const disposable = terminal.onResize(({ cols, rows }) => {
      resizePty(sessionId, cols, rows);
    });

    return () => disposable.dispose();
  }, [sessionId, terminal]);
}
```

Key differences from `useTmuxBridge`:
- No polling — output is pushed via `subscribe()`
- On session switch, replays accumulated buffer
- Keyboard input writes raw data directly to PTY (no special key mapping needed — xterm.js already sends the right escape sequences)
- Resize calls `resizePty` directly

- [ ] **Step 2: Commit**

```bash
git add src/hooks/usePtyBridge.ts
git commit -m "feat: add usePtyBridge hook for real-time PTY-xterm connection"
```

---

### Task 8: Update Terminal.tsx

**Files:**
- Modify: `src/components/Terminal/Terminal.tsx`

- [ ] **Step 1: Swap bridge hook and session ID**

Replace the tmux bridge usage with the PTY bridge. Key changes:
- Import `usePtyBridge` instead of `useTmuxBridge`
- Use `activeSession?.session.id ?? null` (number) instead of `activeSession?.session.tmux_session_name ?? null` (string)
- Pass numeric `sessionId` to `usePtyBridge`

Updated file:

```typescript
import { useEffect, useRef, useState, useCallback } from "react";
import { useSessionStore } from "../../stores/sessionStore";
import { usePtyBridge } from "../../hooks/usePtyBridge";
import type { Terminal as XTermType } from "@xterm/xterm";

export function Terminal() {
  const terminalRef = useRef<HTMLDivElement>(null);
  const [term, setTerm] = useState<XTermType | null>(null);
  const activeSession = useSessionStore((s) => s.getActiveSession());
  const sessionId = activeSession?.session.id ?? null;

  // Initialize xterm.js instance
  useEffect(() => {
    if (!terminalRef.current) return;

    const el = terminalRef.current;
    let xterm: XTermType | null = null;
    let disposed = false;

    const init = async () => {
      const { Terminal: XTerm } = await import("@xterm/xterm");
      const { FitAddon } = await import("@xterm/addon-fit");

      if (disposed) return;

      xterm = new XTerm({
        cursorBlink: true,
        fontSize: 13,
        fontFamily: '"JetBrains Mono", "Fira Code", monospace',
        theme: {
          background: "#111113",
          foreground: "#e4e4e7",
          cursor: "#6366f1",
          selectionBackground: "#6366f140",
        },
        allowProposedApi: true,
      });

      const fitAddon = new FitAddon();
      xterm.loadAddon(fitAddon);
      xterm.open(el);
      fitAddon.fit();

      const resizeObserver = new ResizeObserver(() => {
        if (!disposed) fitAddon.fit();
      });
      resizeObserver.observe(el);

      setTerm(xterm);

      return () => {
        resizeObserver.disconnect();
      };
    };

    const cleanupPromise = init();

    return () => {
      disposed = true;
      cleanupPromise.then((cleanup) => cleanup?.());
      if (xterm) {
        xterm.dispose();
        setTerm(null);
      }
    };
  }, []);

  // Wire up the PTY bridge
  usePtyBridge({
    sessionId,
    terminal: term,
  });

  // Focus terminal on click
  const handleClick = useCallback(() => {
    term?.focus();
  }, [term]);

  if (!sessionId) {
    return (
      <div className="flex flex-1 items-center justify-center text-zinc-500">
        <div className="text-center">
          <p className="text-lg font-medium">No active session</p>
          <p className="mt-1 text-sm">
            Create a new session from the sidebar to get started.
          </p>
        </div>
      </div>
    );
  }

  return (
    <div
      ref={terminalRef}
      onClick={handleClick}
      className="flex-1 bg-surface-1 p-1"
      style={{ minHeight: 0 }}
    />
  );
}
```

Changes:
- Line 3: `usePtyBridge` replaces `useTmuxBridge`
- Line 10: `session.id` replaces `session.tmux_session_name`
- Line 68-71: Removed `prevSessionRef` session-switch reset (handled inside `usePtyBridge`)
- Line 78: `usePtyBridge` replaces `useTmuxBridge`

- [ ] **Step 2: Commit**

```bash
git add src/components/Terminal/Terminal.tsx
git commit -m "refactor: Terminal uses usePtyBridge instead of useTmuxBridge"
```

---

### Task 9: Update sessionStore.ts — integrate ptyManager

**Files:**
- Modify: `src/stores/sessionStore.ts`

- [ ] **Step 1: Add PTY spawn/kill to store actions**

The store needs to:
- After `create_session` returns: spawn PTY in the session's working directory
- Before `stop_session`: kill the PTY
- On `removeSession`: kill PTY if alive

```typescript
import { create } from "zustand";
import { invoke } from "@tauri-apps/api/core";
import type { Repo, Session, RepoWithSessions } from "../types/session";
import { spawnPty, killPty, killAll } from "../services/ptyManager";

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

    // Kill all PTYs on app close to avoid orphaned processes
    window.addEventListener("beforeunload", () => killAll());
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

      // Resolve working directory: worktree path, or fall back to repo path
      const { repos } = get();
      const repo = repos.find((r) => r.repo.id === repoId)?.repo;
      const cwd = session.worktree_path || repo?.path || ".";

      // Spawn PTY in the session's working directory.
      // Use reasonable defaults; usePtyBridge will resize to actual terminal dims on mount.
      spawnPty(session.id, cwd, 80, 24, "claude");

      const updatedRepos = await invoke<RepoWithSessions[]>("list_repos");
      set({ repos: updatedRepos, activeSessionId: session.id });
    } catch (e) {
      set({ error: String(e) });
      throw e;
    }
  },

  stopSession: async (sessionId) => {
    try {
      killPty(sessionId);
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
      killPty(sessionId);
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

Key changes:
- Import `spawnPty`, `killPty` from ptyManager
- `createSession`: after Rust returns the session, spawns PTY with `cwd` = worktree_path or repo.path, and sends "claude" as the initial agent command
- `stopSession`: kills PTY before updating DB
- `removeSession`: kills PTY before deleting from DB

- [ ] **Step 2: Commit**

```bash
git add src/stores/sessionStore.ts
git commit -m "feat: integrate ptyManager into session store for PTY lifecycle"
```

---

### Task 10: Delete useTmuxBridge.ts

**Files:**
- Delete: `src/hooks/useTmuxBridge.ts`

- [ ] **Step 1: Delete the file**

```bash
rm /Users/yuchenliu/Documents/otte/src/hooks/useTmuxBridge.ts
```

- [ ] **Step 2: Verify no remaining tmux references in frontend**

```bash
cd /Users/yuchenliu/Documents/otte && grep -r "tmux" src/ --include="*.ts" --include="*.tsx"
```

Expected: No matches.

- [ ] **Step 3: Commit**

```bash
git add -A src/hooks/useTmuxBridge.ts
git commit -m "refactor: delete useTmuxBridge.ts, fully replaced by usePtyBridge"
```

---

## Chunk 3: Verification

### Task 11: Build verification

- [ ] **Step 1: TypeScript check**

```bash
cd /Users/yuchenliu/Documents/otte && bun run build
```

Expected: No TypeScript errors, Vite build succeeds.

- [ ] **Step 2: Rust check**

```bash
cd /Users/yuchenliu/Documents/otte/src-tauri && cargo check
```

Expected: No Rust errors or warnings.

- [ ] **Step 3: Full Tauri build**

```bash
cd /Users/yuchenliu/Documents/otte && bun tauri build
```

Expected: Builds successfully.

- [ ] **Step 4: Verify no tmux references remain in source code**

```bash
cd /Users/yuchenliu/Documents/otte && grep -r "tmux" src/ src-tauri/src/ --include="*.ts" --include="*.tsx" --include="*.rs"
```

Expected: No matches.

- [ ] **Step 5: Update documentation references**

Search for tmux references in docs/wiki/CLAUDE.md and update them:

```bash
grep -r "tmux" CLAUDE.md wiki/ docs/ --include="*.md"
```

Update `CLAUDE.md` to reflect:
- Session = PTY process + git worktree (not tmux session)
- Agent-agnostic: Communication via native PTY (not tmux send-keys/capture-pane)
- Remove any tmux mentions from architecture description

- [ ] **Step 6: Manual smoke test**

```bash
cd /Users/yuchenliu/Documents/otte && bun tauri dev
```

Test checklist:
1. App launches without errors
2. Import a repo
3. Create a session → terminal shows shell prompt within 1 second
4. "claude" command is sent to the shell
5. Type in terminal → characters appear immediately (no 150ms delay)
6. Resize window → terminal content reflows
7. Stop session → process terminates, status updates
8. Switch between sessions → output is preserved

- [ ] **Step 7: Final commit (if any fixups needed)**

```bash
git add -A
git commit -m "fix: address build issues from PTY migration"
```
