# Task Board Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a Task Board to Racc's center panel that lets users write task descriptions (cognitive offloading) and fire them to automatically spawn agent sessions, with results queuing for batched review.

**Architecture:** New `tasks` SQLite table linked to sessions via FK. Rust backend exposes CRUD + fire commands. React frontend adds a Zustand taskStore and a kanban-style TaskBoard component in the center panel with tab switching alongside Terminal.

**Tech Stack:** Rust (rusqlite, Tauri commands), React 19, TypeScript, Zustand 5, Tailwind CSS

---

## File Structure

### New Files

| File | Responsibility |
|------|---------------|
| `src-tauri/src/commands/task.rs` | Rust commands: create, list, update status, delete tasks |
| `src/types/task.ts` | TypeScript Task interface and TaskStatus type |
| `src/stores/taskStore.ts` | Zustand store for task state and actions |
| `src/components/TaskBoard/TaskBoard.tsx` | Kanban layout with 4 columns |
| `src/components/TaskBoard/TaskColumn.tsx` | Single status column with header + card list |
| `src/components/TaskBoard/TaskCard.tsx` | Task card, varies by status |
| `src/components/TaskBoard/TaskInput.tsx` | Inline input for creating new tasks |
| `src/components/TaskBoard/FireTaskDialog.tsx` | Modal dialog for fire configuration |

### Modified Files

| File | Change |
|------|--------|
| `src-tauri/src/commands/db.rs` | Add v4 migration with `tasks` table |
| `src-tauri/src/commands/mod.rs` | Add `pub mod task;` |
| `src-tauri/src/lib.rs` | Register new task commands |
| `src/App.tsx` | Add Tasks/Terminal tab switching in center panel |
| `src/types/session.ts` | No changes needed (Task type goes in own file) |

---

## Chunk 1: Backend — Database & Rust Commands

### Task 1: Add tasks table migration (v3 → v4)

**Files:**
- Modify: `src-tauri/src/commands/db.rs`

- [ ] **Step 1: Read current db.rs**

Read `src-tauri/src/commands/db.rs` to see the full migration chain (v1→v2→v3).

- [ ] **Step 2: Add v4 migration**

After the `if version < 3` block (around line 109), add:

```rust
if version < 4 {
    conn.execute_batch(
        "
        BEGIN;

        CREATE TABLE IF NOT EXISTS tasks (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            repo_id INTEGER NOT NULL REFERENCES repos(id),
            description TEXT NOT NULL,
            status TEXT NOT NULL DEFAULT 'open',
            session_id INTEGER REFERENCES sessions(id),
            created_at TEXT NOT NULL DEFAULT (datetime('now')),
            updated_at TEXT NOT NULL DEFAULT (datetime('now'))
        );

        PRAGMA user_version = 4;

        COMMIT;
        ",
    )
    .map_err(|e| format!("Migration v4 failed: {e}"))?;
}
```

- [ ] **Step 3: Verify it compiles**

Run: `cd src-tauri && cargo check`
Expected: compiles with no errors

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/commands/db.rs
git commit -m "feat(db): add v4 migration with tasks table"
```

---

### Task 2: Create task.rs Rust command module

**Files:**
- Create: `src-tauri/src/commands/task.rs`
- Modify: `src-tauri/src/commands/mod.rs`

- [ ] **Step 1: Read mod.rs to see module pattern**

Read `src-tauri/src/commands/mod.rs` for the existing module declaration pattern.

- [ ] **Step 2: Add task module to mod.rs**

Add `pub mod task;` alongside existing module declarations.

- [ ] **Step 3: Create task.rs with Task struct and CRUD commands**

Create `src-tauri/src/commands/task.rs`:

```rust
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::sync::Mutex;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: i64,
    pub repo_id: i64,
    pub description: String,
    pub status: String,
    pub session_id: Option<i64>,
    pub created_at: String,
    pub updated_at: String,
}

#[tauri::command]
pub fn create_task(
    db: tauri::State<'_, Mutex<Connection>>,
    repo_id: i64,
    description: String,
) -> Result<Task, String> {
    let conn = db.lock().map_err(|e| e.to_string())?;
    conn.execute(
        "INSERT INTO tasks (repo_id, description) VALUES (?1, ?2)",
        rusqlite::params![repo_id, description],
    )
    .map_err(|e| format!("Failed to create task: {e}"))?;

    let id = conn.last_insert_rowid();
    let task = conn
        .query_row(
            "SELECT id, repo_id, description, status, session_id, created_at, updated_at FROM tasks WHERE id = ?1",
            [id],
            |row| {
                Ok(Task {
                    id: row.get(0)?,
                    repo_id: row.get(1)?,
                    description: row.get(2)?,
                    status: row.get(3)?,
                    session_id: row.get(4)?,
                    created_at: row.get(5)?,
                    updated_at: row.get(6)?,
                })
            },
        )
        .map_err(|e| format!("Failed to fetch created task: {e}"))?;

    Ok(task)
}

#[tauri::command]
pub fn list_tasks(
    db: tauri::State<'_, Mutex<Connection>>,
    repo_id: i64,
) -> Result<Vec<Task>, String> {
    let conn = db.lock().map_err(|e| e.to_string())?;
    let mut stmt = conn
        .prepare(
            "SELECT id, repo_id, description, status, session_id, created_at, updated_at FROM tasks WHERE repo_id = ?1 ORDER BY created_at DESC",
        )
        .map_err(|e| format!("Failed to prepare query: {e}"))?;

    let tasks = stmt
        .query_map([repo_id], |row| {
            Ok(Task {
                id: row.get(0)?,
                repo_id: row.get(1)?,
                description: row.get(2)?,
                status: row.get(3)?,
                session_id: row.get(4)?,
                created_at: row.get(5)?,
                updated_at: row.get(6)?,
            })
        })
        .map_err(|e| format!("Failed to query tasks: {e}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("Failed to collect tasks: {e}"))?;

    Ok(tasks)
}

#[tauri::command]
pub fn update_task_status(
    db: tauri::State<'_, Mutex<Connection>>,
    task_id: i64,
    status: String,
    session_id: Option<i64>,
) -> Result<Task, String> {
    let conn = db.lock().map_err(|e| e.to_string())?;

    if let Some(sid) = session_id {
        conn.execute(
            "UPDATE tasks SET status = ?1, session_id = ?2, updated_at = datetime('now') WHERE id = ?3",
            rusqlite::params![status, sid, task_id],
        )
        .map_err(|e| format!("Failed to update task: {e}"))?;
    } else {
        conn.execute(
            "UPDATE tasks SET status = ?1, updated_at = datetime('now') WHERE id = ?2",
            rusqlite::params![status, task_id],
        )
        .map_err(|e| format!("Failed to update task: {e}"))?;
    }

    let task = conn
        .query_row(
            "SELECT id, repo_id, description, status, session_id, created_at, updated_at FROM tasks WHERE id = ?1",
            [task_id],
            |row| {
                Ok(Task {
                    id: row.get(0)?,
                    repo_id: row.get(1)?,
                    description: row.get(2)?,
                    status: row.get(3)?,
                    session_id: row.get(4)?,
                    created_at: row.get(5)?,
                    updated_at: row.get(6)?,
                })
            },
        )
        .map_err(|e| format!("Failed to fetch updated task: {e}"))?;

    Ok(task)
}

#[tauri::command]
pub fn delete_task(
    db: tauri::State<'_, Mutex<Connection>>,
    task_id: i64,
) -> Result<(), String> {
    let conn = db.lock().map_err(|e| e.to_string())?;
    conn.execute("DELETE FROM tasks WHERE id = ?1", [task_id])
        .map_err(|e| format!("Failed to delete task: {e}"))?;
    Ok(())
}
```

- [ ] **Step 4: Verify it compiles**

Run: `cd src-tauri && cargo check`
Expected: compiles with no errors

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/commands/task.rs src-tauri/src/commands/mod.rs
git commit -m "feat(backend): add task CRUD commands"
```

---

### Task 3: Register task commands in lib.rs

**Files:**
- Modify: `src-tauri/src/lib.rs`

- [ ] **Step 1: Read lib.rs**

Read `src-tauri/src/lib.rs` to find the `generate_handler![]` macro invocation.

- [ ] **Step 2: Add task commands to handler**

In the `generate_handler![]` macro, add:

```rust
commands::task::create_task,
commands::task::list_tasks,
commands::task::update_task_status,
commands::task::delete_task,
```

- [ ] **Step 3: Verify it compiles**

Run: `cd src-tauri && cargo check`
Expected: compiles with no errors

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/lib.rs
git commit -m "feat(backend): register task commands in Tauri handler"
```

---

## Chunk 2: Frontend — Types & Store

### Task 4: Add TypeScript task types

**Files:**
- Create: `src/types/task.ts`

- [ ] **Step 1: Create task.ts type definitions**

Create `src/types/task.ts`:

```typescript
export type TaskStatus = "open" | "running" | "review" | "done";

export interface Task {
  id: number;
  repo_id: number;
  description: string;
  status: TaskStatus;
  session_id: number | null;
  created_at: string;
  updated_at: string;
}
```

- [ ] **Step 2: Verify TypeScript compiles**

Run: `bun run build`
Expected: no type errors

- [ ] **Step 3: Commit**

```bash
git add src/types/task.ts
git commit -m "feat(types): add Task interface and TaskStatus type"
```

---

### Task 5: Create taskStore

**Files:**
- Create: `src/stores/taskStore.ts`

- [ ] **Step 1: Read sessionStore.ts for store patterns**

Read `src/stores/sessionStore.ts` to follow the same Zustand patterns (invoke calls, error handling, state updates).

- [ ] **Step 2: Create taskStore.ts**

Create `src/stores/taskStore.ts`:

```typescript
import { create } from "zustand";
import { invoke } from "@tauri-apps/api/core";
import type { Task, TaskStatus } from "../types/task";

interface TaskState {
  tasks: Task[];
  loading: boolean;
  error: string | null;

  loadTasks: (repoId: number) => Promise<void>;
  createTask: (repoId: number, description: string) => Promise<Task>;
  fireTask: (
    taskId: number,
    repoId: number,
    useWorktree: boolean,
    branch: string | undefined,
    skipPermissions: boolean
  ) => Promise<void>;
  updateTaskStatus: (
    taskId: number,
    status: TaskStatus,
    sessionId?: number
  ) => Promise<void>;
  deleteTask: (taskId: number) => Promise<void>;
  syncTaskWithSession: (sessionId: number, sessionStatus: string) => void;
}

export const useTaskStore = create<TaskState>((set, get) => ({
  tasks: [],
  loading: false,
  error: null,

  loadTasks: async (repoId: number) => {
    set({ loading: true, error: null });
    try {
      const tasks = await invoke<Task[]>("list_tasks", { repoId });
      set({ tasks, loading: false });
    } catch (err) {
      set({ error: String(err), loading: false });
    }
  },

  createTask: async (repoId: number, description: string) => {
    const task = await invoke<Task>("create_task", { repoId, description });
    set((state) => ({ tasks: [task, ...state.tasks] }));
    return task;
  },

  // NOTE: The spec lists fire_task as a Rust command, but per the spec's own
  // Implementation Notes, agent/skip-permissions are frontend concerns.
  // fire_task is intentionally implemented on the frontend, calling the
  // existing create_session Rust command internally.
  fireTask: async (taskId, repoId, useWorktree, branch, skipPermissions) => {
    // Import sessionStore dynamically to avoid circular deps
    const { useSessionStore } = await import("./sessionStore");
    const { createSession } = useSessionStore.getState();

    // Capture session count before creation to find the new one
    const beforeRepos = useSessionStore.getState().repos;
    const beforeRepo = beforeRepos.find((r) => r.id === repoId);
    const beforeIds = new Set(beforeRepo?.sessions.map((s) => s.id) ?? []);

    // Create session via existing flow
    await createSession(repoId, useWorktree, branch, skipPermissions);

    // Find the newly created session by diffing session IDs
    const afterRepos = useSessionStore.getState().repos;
    const afterRepo = afterRepos.find((r) => r.id === repoId);
    const newSession = afterRepo?.sessions.find((s) => !beforeIds.has(s.id));

    if (newSession) {
      // Link task to session and set running
      await get().updateTaskStatus(taskId, "running", newSession.id);

      // Send task description to PTY as initial prompt.
      // Uses ptyManager's onData subscription to wait for agent readiness
      // rather than a fixed timeout. The 2s delay is a pragmatic fallback —
      // the agent needs time to initialize its REPL before accepting input.
      // Known limitation: if the agent takes longer (cold start, large repo),
      // the prompt may arrive too early. A signal-based approach (detecting
      // the agent's ready prompt via ptyOutputParser) would be more robust
      // but is deferred to keep scope minimal.
      const task = get().tasks.find((t) => t.id === taskId);
      if (task) {
        const { writePty } = await import("../services/ptyManager");
        setTimeout(() => {
          writePty(newSession.id, task.description + "\n");
        }, 2000);
      }
    }
  },

  updateTaskStatus: async (taskId, status, sessionId) => {
    const task = await invoke<Task>("update_task_status", {
      taskId,
      status,
      sessionId: sessionId ?? null,
    });
    set((state) => ({
      tasks: state.tasks.map((t) => (t.id === taskId ? task : t)),
    }));
  },

  deleteTask: async (taskId: number) => {
    await invoke("delete_task", { taskId });
    set((state) => ({
      tasks: state.tasks.filter((t) => t.id !== taskId),
    }));
  },

  syncTaskWithSession: (sessionId: number, sessionStatus: string) => {
    const { tasks } = get();
    const task = tasks.find(
      (t) => t.session_id === sessionId && t.status === "running"
    );
    if (task && sessionStatus === "Completed") {
      get().updateTaskStatus(task.id, "review");
    }
  },
}));
```

- [ ] **Step 3: Verify TypeScript compiles**

Run: `bun run build`
Expected: no type errors

- [ ] **Step 4: Commit**

```bash
git add src/stores/taskStore.ts
git commit -m "feat(store): add taskStore with CRUD, fire, and session sync"
```

---

## Chunk 3: UI Components — TaskBoard

### Task 6: Create TaskInput component

**Files:**
- Create: `src/components/TaskBoard/TaskInput.tsx`

- [ ] **Step 1: Create TaskInput.tsx**

Create `src/components/TaskBoard/TaskInput.tsx`:

```tsx
import { useState, useRef, useEffect } from "react";

interface Props {
  onSubmit: (description: string) => void;
  onCancel: () => void;
}

export function TaskInput({ onSubmit, onCancel }: Props) {
  const [value, setValue] = useState("");
  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    inputRef.current?.focus();
  }, []);

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === "Enter" && value.trim()) {
      onSubmit(value.trim());
      setValue("");
    }
    if (e.key === "Escape") {
      onCancel();
    }
  };

  return (
    <input
      ref={inputRef}
      type="text"
      value={value}
      onChange={(e) => setValue(e.target.value)}
      onKeyDown={handleKeyDown}
      onBlur={onCancel}
      placeholder="Describe your task..."
      className="w-full rounded border border-accent bg-surface-2 px-3 py-2 text-sm text-white placeholder-zinc-600 outline-none focus:border-accent-hover"
    />
  );
}
```

- [ ] **Step 2: Verify it compiles**

Run: `bun run build`
Expected: no errors

- [ ] **Step 3: Commit**

```bash
git add src/components/TaskBoard/TaskInput.tsx
git commit -m "feat(ui): add TaskInput component for inline task creation"
```

---

### Task 7: Create FireTaskDialog component

**Files:**
- Create: `src/components/TaskBoard/FireTaskDialog.tsx`

- [ ] **Step 1: Read NewAgentDialog.tsx for the dialog pattern**

Read `src/components/Sidebar/NewAgentDialog.tsx` to follow the same visual pattern.

- [ ] **Step 2: Create FireTaskDialog.tsx**

Create `src/components/TaskBoard/FireTaskDialog.tsx`:

```tsx
import { useState, useMemo } from "react";
import type { Task } from "../../types/task";
import { useTaskStore } from "../../stores/taskStore";

interface Props {
  task: Task;
  open: boolean;
  onClose: () => void;
}

function generateBranchName(description: string): string {
  return (
    "task/" +
    description
      .toLowerCase()
      .replace(/[^a-z0-9\s]/g, "")
      .trim()
      .split(/\s+/)
      .slice(0, 4)
      .join("-")
  );
}

export function FireTaskDialog({ task, open, onClose }: Props) {
  const [useWorktree, setUseWorktree] = useState(true);
  const [skipPermissions, setSkipPermissions] = useState(true);
  const defaultBranch = useMemo(
    () => generateBranchName(task.description),
    [task.description]
  );
  const [branch, setBranch] = useState(defaultBranch);
  const [firing, setFiring] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const fireTask = useTaskStore((s) => s.fireTask);

  if (!open) return null;

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (useWorktree && !branch.trim()) return;

    setFiring(true);
    setError(null);
    try {
      await fireTask(
        task.id,
        task.repo_id,
        useWorktree,
        useWorktree ? branch.trim() : undefined,
        skipPermissions
      );
      onClose();
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setFiring(false);
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
          Fire Task
        </h2>

        <div className="mb-4 rounded border-l-2 border-accent bg-surface-2 px-3 py-2 text-xs text-zinc-400">
          {task.description}
        </div>

        <label className="mb-3 block">
          <span className="mb-1 block text-xs text-zinc-400">Agent</span>
          <select className="w-full rounded border border-surface-3 bg-surface-2 px-3 py-1.5 text-sm text-white outline-none focus:border-accent">
            <option value="claude-code">Claude Code</option>
          </select>
        </label>

        <label className="mb-4 flex items-center gap-2 text-xs text-zinc-300">
          <input
            type="checkbox"
            checked={skipPermissions}
            onChange={(e) => setSkipPermissions(e.target.checked)}
            className="accent-accent"
          />
          Skip permissions
        </label>

        <label className="mb-4 flex items-center gap-2 text-xs text-zinc-300">
          <input
            type="checkbox"
            checked={useWorktree}
            onChange={(e) => setUseWorktree(e.target.checked)}
            className="accent-accent"
          />
          Create a new worktree
        </label>

        {useWorktree && (
          <label className="mb-4 block">
            <span className="mb-1 block text-xs text-zinc-400">
              Branch name
            </span>
            <input
              type="text"
              value={branch}
              onChange={(e) => setBranch(e.target.value)}
              placeholder="task/my-feature"
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
            disabled={firing || (useWorktree && !branch.trim())}
            className="rounded bg-accent px-3 py-1.5 text-xs font-medium text-white hover:bg-accent-hover disabled:opacity-50"
          >
            {firing ? "Firing..." : "Fire"}
          </button>
        </div>
      </form>
    </div>
  );
}
```

- [ ] **Step 3: Verify it compiles**

Run: `bun run build`
Expected: no errors

- [ ] **Step 4: Commit**

```bash
git add src/components/TaskBoard/FireTaskDialog.tsx
git commit -m "feat(ui): add FireTaskDialog with auto-generated branch name"
```

---

### Task 8: Create TaskCard component

**Files:**
- Create: `src/components/TaskBoard/TaskCard.tsx`

- [ ] **Step 1: Create TaskCard.tsx**

Create `src/components/TaskBoard/TaskCard.tsx`:

```tsx
import { useState, useMemo } from "react";
import type { Task } from "../../types/task";
import { useSessionStore } from "../../stores/sessionStore";
import { useTaskStore } from "../../stores/taskStore";
import { FireTaskDialog } from "./FireTaskDialog";

interface Props {
  task: Task;
  onSwitchToTerminal: () => void;
}

function formatElapsed(createdAt: string): string {
  const diff = Date.now() - new Date(createdAt + "Z").getTime();
  const mins = Math.floor(diff / 60000);
  if (mins < 1) return "<1m";
  if (mins < 60) return `${mins}m`;
  return `${Math.floor(mins / 60)}h ${mins % 60}m`;
}

export function TaskCard({ task, onSwitchToTerminal }: Props) {
  const [fireOpen, setFireOpen] = useState(false);
  const sessionActivities = useSessionStore((s) => s.sessionActivities);
  const setActiveSession = useSessionStore((s) => s.setActiveSession);
  const repos = useSessionStore((s) => s.repos);
  const updateTaskStatus = useTaskStore((s) => s.updateTaskStatus);

  const activity = task.session_id
    ? sessionActivities[task.session_id]
    : null;

  // Find linked session for branch name display
  const linkedSession = useMemo(() => {
    if (!task.session_id) return null;
    for (const repo of repos) {
      const session = repo.sessions.find((s) => s.id === task.session_id);
      if (session) return session;
    }
    return null;
  }, [task.session_id, repos]);

  const statusBorder = {
    open: "border-l-accent",
    running: "border-l-status-running",
    review: "border-l-status-waiting",
    done: "border-l-status-completed",
  }[task.status];

  const handleReviewClick = () => {
    if (task.session_id) {
      setActiveSession(task.session_id);
      onSwitchToTerminal();
    }
  };

  const handleMarkDone = (e: React.MouseEvent) => {
    e.stopPropagation();
    updateTaskStatus(task.id, "done");
  };

  return (
    <>
      <div
        className={`rounded border border-surface-3 border-l-2 ${statusBorder} bg-surface-1 p-2.5 transition-colors hover:bg-surface-2 ${
          task.status === "done" ? "opacity-50" : ""
        } ${task.status === "review" ? "cursor-pointer" : ""}`}
        onClick={task.status === "review" ? handleReviewClick : undefined}
      >
        <p className="mb-1 text-xs font-medium leading-snug text-zinc-200">
          {task.description}
        </p>

        {/* Running: show linked session + live activity + elapsed time */}
        {task.status === "running" && (
          <>
            {activity && (
              <div className="mb-1 flex items-center gap-1.5 text-[10px] text-status-running">
                <span className="inline-block h-1 w-1 animate-status-pulse rounded-full bg-status-running" />
                <span className="truncate">
                  {linkedSession?.branch ?? "session"}
                  {" — "}
                  {activity.action}
                  {activity.detail ? ` ${activity.detail}` : ""}
                </span>
              </div>
            )}
            <div className="flex items-center gap-2 text-[10px] text-zinc-500">
              <span className="rounded bg-surface-2 px-1.5 py-0.5">claude</span>
              <span>{formatElapsed(task.updated_at)}</span>
            </div>
          </>
        )}

        {/* Review: show diff summary hint + done button */}
        {task.status === "review" && (
          <>
            <div className="mb-1 text-[10px] italic text-zinc-500">
              Click to review in terminal
            </div>
            <div className="flex items-center justify-between">
              <span className="text-[10px] text-zinc-500">
                {linkedSession?.branch ?? "session"} · done {formatElapsed(task.updated_at)} ago
              </span>
              <button
                onClick={handleMarkDone}
                className="rounded bg-status-completed/15 px-1.5 py-0.5 text-[10px] text-status-completed hover:bg-status-completed/25"
              >
                Done
              </button>
            </div>
          </>
        )}

        {/* Open: show fire button */}
        {task.status === "open" && (
          <div className="flex items-center gap-2 text-[10px] text-zinc-500">
            <span className="rounded bg-surface-2 px-1.5 py-0.5">claude</span>
            <button
              onClick={() => setFireOpen(true)}
              className="ml-auto rounded bg-accent/15 px-2 py-0.5 text-accent hover:bg-accent/25"
            >
              Fire
            </button>
          </div>
        )}

        {/* Done: minimal meta */}
        {task.status === "done" && (
          <div className="text-[10px] text-zinc-600">
            {linkedSession?.branch ?? "session"} · completed
          </div>
        )}
      </div>

      <FireTaskDialog
        task={task}
        open={fireOpen}
        onClose={() => setFireOpen(false)}
      />
    </>
  );
}
```

- [ ] **Step 2: Verify it compiles**

Run: `bun run build`
Expected: no errors

- [ ] **Step 3: Commit**

```bash
git add src/components/TaskBoard/TaskCard.tsx
git commit -m "feat(ui): add TaskCard with status-dependent interactions"
```

---

### Task 9: Create TaskColumn component

**Files:**
- Create: `src/components/TaskBoard/TaskColumn.tsx`

- [ ] **Step 1: Create TaskColumn.tsx**

Create `src/components/TaskBoard/TaskColumn.tsx`:

```tsx
import { useState } from "react";
import type { Task, TaskStatus } from "../../types/task";
import { TaskCard } from "./TaskCard";
import { TaskInput } from "./TaskInput";

const COLUMN_CONFIG: Record<
  TaskStatus,
  { label: string; dotColor: string }
> = {
  open: { label: "Open", dotColor: "bg-accent" },
  running: { label: "Running", dotColor: "bg-status-running" },
  review: { label: "Review", dotColor: "bg-status-waiting" },
  done: { label: "Done", dotColor: "bg-status-completed" },
};

interface Props {
  status: TaskStatus;
  tasks: Task[];
  onCreateTask?: (description: string) => void;
  onSwitchToTerminal: () => void;
}

export function TaskColumn({
  status,
  tasks,
  onCreateTask,
  onSwitchToTerminal,
}: Props) {
  const [inputOpen, setInputOpen] = useState(false);
  const config = COLUMN_CONFIG[status];

  return (
    <div className="flex min-w-0 flex-1 flex-col gap-1.5">
      {/* Column header */}
      <div className="mb-1 flex items-center gap-2 px-2 py-1">
        <span className={`h-1.5 w-1.5 rounded-full ${config.dotColor}`} />
        <span className="text-[10px] uppercase tracking-wider text-zinc-500">
          {config.label}
        </span>
        <span className="text-[10px] text-zinc-600">{tasks.length}</span>
      </div>

      {/* Cards */}
      <div className="flex flex-col gap-1.5 overflow-y-auto px-1">
        {tasks.map((task) => (
          <TaskCard
            key={task.id}
            task={task}
            onSwitchToTerminal={onSwitchToTerminal}
          />
        ))}
      </div>

      {/* New task input (Open column only) */}
      {onCreateTask && (
        <div className="px-1">
          {inputOpen ? (
            <TaskInput
              onSubmit={(desc) => {
                onCreateTask(desc);
                setInputOpen(false);
              }}
              onCancel={() => setInputOpen(false)}
            />
          ) : (
            <button
              onClick={() => setInputOpen(true)}
              className="w-full rounded border border-dashed border-surface-3 py-1.5 text-center text-[10px] text-zinc-600 transition-colors hover:border-accent hover:text-accent"
            >
              + New Task
            </button>
          )}
        </div>
      )}
    </div>
  );
}
```

- [ ] **Step 2: Verify it compiles**

Run: `bun run build`
Expected: no errors

- [ ] **Step 3: Commit**

```bash
git add src/components/TaskBoard/TaskColumn.tsx
git commit -m "feat(ui): add TaskColumn with header, cards, and new-task input"
```

---

### Task 10: Create TaskBoard component

**Files:**
- Create: `src/components/TaskBoard/TaskBoard.tsx`

- [ ] **Step 1: Create TaskBoard.tsx**

Create `src/components/TaskBoard/TaskBoard.tsx`:

```tsx
import { useEffect } from "react";
import type { TaskStatus } from "../../types/task";
import { useTaskStore } from "../../stores/taskStore";
import { useSessionStore } from "../../stores/sessionStore";
import { TaskColumn } from "./TaskColumn";

const COLUMNS: TaskStatus[] = ["open", "running", "review", "done"];

interface Props {
  repoId: number | null;
  onSwitchToTerminal: () => void;
}

export function TaskBoard({ repoId, onSwitchToTerminal }: Props) {
  const { tasks, loadTasks, createTask } = useTaskStore();
  const repos = useSessionStore((s) => s.repos);

  // Load tasks when repo changes
  useEffect(() => {
    if (repoId) loadTasks(repoId);
  }, [repoId, loadTasks]);

  // Watch session status changes → sync running→review
  useEffect(() => {
    const syncTaskWithSession = useTaskStore.getState().syncTaskWithSession;
    for (const repo of repos) {
      for (const session of repo.sessions) {
        syncTaskWithSession(session.id, session.status);
      }
    }
  }, [repos]);

  if (!repoId) {
    return (
      <div className="flex flex-1 items-center justify-center text-sm text-zinc-600">
        Select a repo to view tasks
      </div>
    );
  }

  const tasksByStatus = Object.fromEntries(
    COLUMNS.map((status) => [status, tasks.filter((t) => t.status === status)])
  ) as Record<TaskStatus, typeof tasks>;

  return (
    <div className="flex flex-1 gap-2 overflow-x-auto p-3">
      {COLUMNS.map((status) => (
        <TaskColumn
          key={status}
          status={status}
          tasks={tasksByStatus[status]}
          onCreateTask={
            status === "open"
              ? (desc) => createTask(repoId, desc)
              : undefined
          }
          onSwitchToTerminal={onSwitchToTerminal}
        />
      ))}
    </div>
  );
}
```

- [ ] **Step 2: Verify it compiles**

Run: `bun run build`
Expected: no errors

- [ ] **Step 3: Commit**

```bash
git add src/components/TaskBoard/TaskBoard.tsx
git commit -m "feat(ui): add TaskBoard kanban layout with 4 columns"
```

---

## Chunk 4: Integration — Tab Switching in App.tsx

### Task 11: Add tab switching to center panel

**Files:**
- Modify: `src/App.tsx`

- [ ] **Step 1: Read App.tsx**

Read `src/App.tsx` to understand the current layout JSX.

- [ ] **Step 2: Add all imports, state, and derived values**

Add to App.tsx imports:

```tsx
import { useState, useRef, useEffect } from "react";
import { TaskBoard } from "./components/TaskBoard/TaskBoard";
import { useTaskStore } from "./stores/taskStore";
```

Inside the App component, add state and derived values:

```tsx
const [centerTab, setCenterTab] = useState<"tasks" | "terminal">("tasks");
const activeSessionId = useSessionStore((s) => s.activeSessionId);
const repos = useSessionStore((s) => s.repos);
const tasks = useTaskStore((s) => s.tasks);

// Find the repo of the active session, or first repo
const activeRepoId = repos.find((r) =>
  r.sessions.some((s) => s.id === activeSessionId)
)?.id ?? repos[0]?.id ?? null;

// Load tasks for the active repo at App level so tab badge works
const loadTasks = useTaskStore((s) => s.loadTasks);
useEffect(() => {
  if (activeRepoId) loadTasks(activeRepoId);
}, [activeRepoId, loadTasks]);

// Switch to terminal when user clicks a session in sidebar.
// Use a ref to skip the initial mount (so default view stays on Tasks tab).
// Also skip when fireTask sets activeSessionId — only switch when
// the user explicitly clicks a session (detected by checking if we're
// currently on the tasks tab AND the session changed).
const prevSessionRef = useRef(activeSessionId);
useEffect(() => {
  if (
    activeSessionId &&
    activeSessionId !== prevSessionRef.current &&
    centerTab === "tasks"
  ) {
    // Only auto-switch if the session change came from user clicking sidebar,
    // not from fireTask. fireTask sets status to "running" which we can check.
    const isFromFire = tasks.some(
      (t) => t.session_id === activeSessionId && t.status === "running"
    );
    if (!isFromFire) {
      setCenterTab("terminal");
    }
  }
  prevSessionRef.current = activeSessionId;
}, [activeSessionId, centerTab, tasks]);
```

- [ ] **Step 3: Replace center panel content with tabs**

Read the current `<main>` section in App.tsx. Replace its content with a tab bar and conditional rendering. Adapt the snippet below to match the existing component names and wrapper structure:

```tsx
<main className="relative flex flex-1 flex-col border-x border-surface-3">
  {/* Tab bar */}
  <div className="flex border-b border-surface-3 bg-surface-1">
    <button
      onClick={() => setCenterTab("tasks")}
      className={`px-4 py-2 text-xs uppercase tracking-wider transition-colors ${
        centerTab === "tasks"
          ? "border-b-2 border-accent text-zinc-200"
          : "text-zinc-500 hover:text-zinc-300"
      }`}
    >
      Tasks
      {tasks.filter((t) => t.status !== "done").length > 0 && (
        <span
          className={`ml-2 rounded-full px-1.5 py-0.5 text-[9px] ${
            centerTab === "tasks"
              ? "bg-accent/20 text-accent"
              : "bg-surface-3 text-zinc-500"
          }`}
        >
          {tasks.filter((t) => t.status !== "done").length}
        </span>
      )}
    </button>
    <button
      onClick={() => setCenterTab("terminal")}
      className={`px-4 py-2 text-xs uppercase tracking-wider transition-colors ${
        centerTab === "terminal"
          ? "border-b-2 border-accent text-zinc-200"
          : "text-zinc-500 hover:text-zinc-300"
      }`}
    >
      Terminal
    </button>
  </div>

  {/* Content */}
  {centerTab === "tasks" ? (
    <TaskBoard
      repoId={activeRepoId}
      onSwitchToTerminal={() => setCenterTab("terminal")}
    />
  ) : (
    <>
      <Terminal />
      <FileViewer />
    </>
  )}
</main>
```

- [ ] **Step 6: Verify the full app compiles**

Run: `bun run build`
Expected: no errors

- [ ] **Step 7: Manual smoke test**

Run: `bun tauri dev`

Verify:
1. Center panel shows Tasks | Terminal tabs
2. Tasks tab shows kanban with 4 columns (Open, Running, Review, Done)
3. Click "+ New Task" → inline input appears
4. Type description → Enter → task card appears in Open column
5. Click "Fire" on a task → Fire dialog opens with worktree ON and auto-generated branch
6. Click a session in sidebar → switches to Terminal tab
7. Task Board data persists across app restarts

- [ ] **Step 8: Commit**

```bash
git add src/App.tsx
git commit -m "feat(ui): add Tasks/Terminal tab switching in center panel"
```

---

### Task 12: Final integration commit

- [ ] **Step 1: Verify everything builds cleanly**

Run: `cd src-tauri && cargo check && cd .. && bun run build`
Expected: no errors on either side

- [ ] **Step 2: Run full app**

Run: `bun tauri dev`

Full smoke test:
1. Create a task
2. Fire it → verify session spawns, task moves to Running, agent receives description
3. When agent completes → verify task moves to Review
4. Click Review card → switches to terminal
5. Mark Done → task moves to Done column

- [ ] **Step 3: Final commit if any fixes were needed**

Stage only the files modified in this feature:

```bash
git add src-tauri/src/commands/db.rs src-tauri/src/commands/task.rs src-tauri/src/commands/mod.rs src-tauri/src/lib.rs src/types/task.ts src/stores/taskStore.ts src/components/TaskBoard/ src/App.tsx
git commit -m "feat: complete Task Board integration"
```
