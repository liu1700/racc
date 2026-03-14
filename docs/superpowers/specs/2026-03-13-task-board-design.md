# Task Board Design Spec

**Date:** 2026-03-13
**Status:** Draft
**Feature:** Task Board — cognitive offloading + agent task orchestration

## Problem

When orchestrating multiple AI coding agents, users face two cognitive burdens:

1. **Working memory overload** — juggling multiple ideas, bugs, and features in their head while agents execute
2. **Context switching cost** — manually creating sessions, configuring worktrees, and monitoring each agent

Users need a way to quickly dump thoughts into an external store (cognitive offloading) and have the system handle the orchestration automatically.

## Solution

A Task Board integrated into the center panel as a tab alongside the Terminal. Users write task descriptions with minimal friction (the act of writing IS the offload). Tasks can be "fired" to automatically spawn agent sessions. Results queue up for batched review.

## Design Decisions

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Notes vs Task Board | Task Board only | Writing a task IS cognitive offloading — no separate notes layer needed |
| Task ↔ Session | Task drives Session (1:1) | Fire creates a new session automatically; full automation |
| Lifecycle states | Open → Running → Review → Done | Minimal but complete; no draft/ready distinction |
| UI placement | Center panel tab (Tasks \| Terminal) | Board is primary workspace; firing a task doesn't switch view |
| Task creation | Inline text input, description only | Zero-config creation = lowest cognitive friction |
| Fire action | Launch dialog (like NewAgentDialog) | Reuses existing pattern; worktree ON by default, auto-generated branch name |
| Review action | Click card → switch to Terminal tab | Terminal is the source of truth; no duplicate diff UI in board |

## Lifecycle

```
Open ──[▶ Fire]──→ Running ──[Agent completes]──→ Review ──[User confirms]──→ Done
```

### Open
- User writes a description — that's it
- Inline input at bottom of Open column: click "+ New Task", type, press Enter
- No agent, repo, or config selection at this stage
- This is the cognitive offloading moment — friction must be near zero

### Running
- Fire creates a session (worktree + PTY) and sends the task description to the agent
- Task card shows real-time agent activity via existing PTY Output Parser
- Displays: linked session name, current action (e.g., "Reading src/auth/session.ts"), elapsed time
- Green pulsing dot for preattentive status recognition (<200ms per Treisman)
- User stays on Task Board after firing — no auto-switch to terminal

### Review
- When the linked session's status becomes `Completed`, the task automatically moves to Review
- Card shows change summary: files changed, lines added/removed
- Clicking the card switches to Terminal tab and activates the linked session
- User reviews results in the terminal, then returns to Task Board to mark Done

### Done
- Archived state — card shown with reduced opacity
- Can be filtered/hidden

## Task Creation UX

1. User clicks "+ New Task" at bottom of Open column
2. Inline text input expands in-place (no modal)
3. User types task description
4. Enter → creates task, input collapses
5. Esc → cancels, input collapses
6. Task appears as a card in Open column with ▶ Fire button

## Fire Dialog

Triggered by clicking ▶ Fire on an Open task card. Opens a modal dialog reusing the existing `NewAgentDialog` pattern:

```
┌─────────────────────────────┐
│ 🚀 Fire Task                │
│                             │
│ ┌─────────────────────────┐ │
│ │ Fix the memory leak in  │ │
│ │ WebSocket reconnection  │ │  ← task description (read-only)
│ └─────────────────────────┘ │
│                             │
│ Agent:  [Claude Code    ▾]  │
│ ☑ Skip permissions          │
│ ☑ Create worktree           │  ← default ON (unlike NewAgentDialog)
│ Branch: [task/fix-ws-leak]  │  ← auto-generated from description
│                             │
│         [Cancel] [Fire]     │
└─────────────────────────────┘
```

**Defaults differ from NewAgentDialog:**
- Worktree: ON by default (vs OFF in NewAgentDialog)
- Branch name: auto-generated `task/<keywords>` from task description

**On Fire:**
1. Call `create_session(repo_id, use_worktree=true, branch, skip_permissions)`
2. Update task: `status = 'running'`, `session_id = new_session.id`
3. Spawn PTY, send task description as initial prompt to agent
4. Task card moves from Open to Running column
5. Session appears in left sidebar
6. User remains on Task Board (no view switch)

## Running State — Information Scent

Running task cards display real-time agent activity by subscribing to the existing `sessionActivities` from `sessionStore`:

```
┌──────────────────────────────────┐
│ Fix authentication session       │
│ timeout bug                      │
│ 🟢 fix-auth-bug — Reading        │
│    src/auth/session.ts           │
│ claude · 3m elapsed              │
└──────────────────────────────────┘
```

This reuses the PTY Output Parser infrastructure — no new parsing logic needed. The `TaskCard` component reads from `sessionActivities[task.sessionId]`.

## Review → Done Flow

1. PTY Output Parser detects session completion (exit code 0 or agent goodbye pattern)
2. Session status updates to `Completed`
3. `taskStore` watches session status changes → moves task to Review
4. Review card shows diff summary (from existing `get_diff` Rust command)
5. User clicks Review card → center panel switches to Terminal tab, `activeSessionId` set to task's session
6. User reviews in terminal
7. User switches back to Tasks tab, clicks ✓ Done on the task card
8. Task moves to Done column

## Data Model

New `tasks` table (DB migration v4):

```sql
CREATE TABLE tasks (
  id          INTEGER PRIMARY KEY,
  repo_id     INTEGER NOT NULL REFERENCES repos(id),
  description TEXT NOT NULL,
  status      TEXT NOT NULL DEFAULT 'open',  -- open|running|review|done
  session_id  INTEGER REFERENCES sessions(id),  -- NULL until fired
  created_at  TEXT NOT NULL,
  updated_at  TEXT NOT NULL
);
```

- `session_id` is NULL for Open tasks, set on Fire
- Status is a text enum: `open`, `running`, `review`, `done`
- No priority, labels, or due dates (YAGNI)

## Architecture

### Rust Backend

New module `src-tauri/src/commands/task.rs`:

| Command | Signature | Description |
|---------|-----------|-------------|
| `create_task` | `(repo_id: i64, description: String) → Task` | Insert task with status=open |
| `list_tasks` | `(repo_id: i64) → Vec<Task>` | All tasks for a repo |
| `fire_task` | `(task_id: i64, agent: String, use_worktree: bool, branch: Option<String>, skip_permissions: bool) → Task` | Creates session, links to task, updates status=running |
| `update_task_status` | `(task_id: i64, status: String) → Task` | Transition task status |
| `delete_task` | `(task_id: i64) → ()` | Remove a task |

`fire_task` internally calls existing `create_session` logic — no duplication.

Register commands in `src-tauri/src/lib.rs`.

### Frontend

**New store** `src/stores/taskStore.ts`:
- State: `tasks: Task[]`, `loading`, `error`
- Actions: `loadTasks(repoId)`, `createTask(repoId, description)`, `fireTask(taskId, config)`, `updateTaskStatus(taskId, status)`
- Subscription: watch `sessionStore.repos` for session status changes → auto-transition running→review when session completes

**New components** `src/components/TaskBoard/`:
- `TaskBoard.tsx` — kanban layout with 4 columns, receives tasks from store
- `TaskColumn.tsx` — single column (Open/Running/Review/Done) with header + card list
- `TaskCard.tsx` — card component, varies by status (shows Fire button for Open, activity for Running, review action for Review)
- `FireTaskDialog.tsx` — modal dialog for fire configuration (extends NewAgentDialog pattern)
- `TaskInput.tsx` — inline input component for creating new tasks

**Modified files:**
- `src/App.tsx` — add tab switching (Tasks | Terminal) in center panel
- `src/types/` — add `task.ts` with Task type definition

### Tab Switching Behavior

The center panel gets a tab bar: `Tasks | Terminal`

- Default view: Tasks (the board)
- Clicking a session in sidebar → switches to Terminal tab, activates that session
- Clicking a Review task card → switches to Terminal tab, activates linked session
- Firing a task → stays on Tasks tab
- Tab state is local UI state (not persisted)

## Cognitive Science Foundations

| Principle | Application |
|-----------|-------------|
| **Cognitive offloading** (Risko & Gilbert, 2016) | Task creation as externalization of working memory — zero-friction input |
| **Cowan's 4±1 chunks** | Cap visible tasks per column; collapse/scroll excess |
| **Batched evaluation** | Review column accumulates completed tasks for periodic review sessions |
| **Preattentive processing** (Treisman) | Status encoded by color hue alone — green/amber/blue pop out in <200ms |
| **Information scent** (Pirolli & Card) | Running cards show real-time agent activity — judge progress without context switch |
| **Calm technology** (Weiser & Brown) | Board provides ambient awareness; terminal provides deep focus when needed |

## Implementation Notes

- **Agent/skip-permissions are frontend concerns.** The existing `create_session` Rust command only accepts `(repo_id, use_worktree, branch)`. Agent selection and `--dangerously-skip-permissions` are handled in `sessionStore.ts` when constructing the PTY command. `fire_task` on the Rust side should call `create_session` internally; the frontend `FireTaskDialog` handles PTY command construction, same as `NewAgentDialog`.
- **Sending task description to PTY.** After spawning the PTY, the frontend writes the task description as the initial prompt via `ptyManager.write(sessionId, description)`. This is a new behavior — current sessions wait for manual user input.
- **Session status casing.** The Rust backend writes `"Completed"` (capitalized). The taskStore watcher that transitions tasks from running→review must match against `"Completed"`, not `"completed"`.

## Out of Scope

- Task priority / ordering
- Labels / tags / categories
- Due dates
- Task dependencies
- Drag-and-drop between columns
- Multi-session per task
- Task templates
- Bulk operations
