# Feature Specification

[< Home](Home.md) | [< Product Vision](Product-Vision.md)

## P0: Must-Have (MVP)

These features define the minimum viable product. Without any one of them, the product doesn't solve the core problem.

### 1. Multi-Session Dashboard

The main interface showing all active agent sessions as status cards. Designed around Cowan's working memory limit of 4±1 chunks — see [Cognitive Design Research](Cognitive-Design-Research.md).

**Each card = one cognitive chunk:**
- Status color dot (preattentive pop-out for instant triage)
- Current task description / micro-summary
- Runtime duration and progress indicator
- Token consumption (input/output breakdown)
- Latest terminal output (truncated) displayed inline in the sidebar — **implemented**: real-time PTY output capture per session
- Associated git branch and worktree path

**Categorical grouping:** Sessions are ordered by status priority (error/blocked → running → completed) so "needs attention" items always surface first. When 10 agents are grouped into 3 status categories, the developer holds 3 chunks rather than 10.

**Key actions:**
- One-click new session creation (auto-creates worktree + spawns PTY + starts agent)
- Stop / terminate sessions
- Quick switch between sessions (with PTY output buffer replay)

### 2. Real-Time Token Usage Tracking

This is the **#1 user pain point** — the community has independently built 7+ monitoring tools, proving urgency.

**Per-session:**
- Token consumption (input/output breakdown)

**Global (status bar):**
- Total tokens across all sessions
- Weekly token usage
- Session count by status

**MVP approach:** Read Claude Code's local usage data files. Token counts only — no USD cost estimation (irrelevant for subscription users like Claude Max). Support for other agents' usage data in later versions.

### 3. Task Board — Cognitive Offloading & Agent Orchestration (implemented)

A kanban-style task board integrated into the center panel for cognitive offloading and automated agent orchestration. Users write task descriptions (the act of writing IS the cognitive offload) and "fire" them to automatically spawn agent sessions.

**Lifecycle:** Open → Working → Closed

- **Open:** User writes a task description via inline textarea (multiline, wraps) — zero-config, minimal friction. Open tasks are editable: click the description to inline-edit before firing. Users can attach images via clipboard paste (Cmd+V) or file picker button — thumbnails display below the textarea with per-image delete
- **Working:** System auto-creates a session (worktree + PTY), sends task description as initial prompt. If images are attached, their absolute file paths are appended to the prompt so the terminal agent can read them. Card shows real-time agent activity via PTY Output Parser (information scent)
- **Closed:** Session completes or is removed → task automatically moves to Closed

**Image Attachments:** Images are saved to `{repo_path}/.racc/images/` as files (named `{taskId}-{timestamp}-{index}.{ext}`). Draft images use temporary names and are renamed after task creation. TaskCards display small (32×32) thumbnails of attached images. On fire, the prompt sent to the terminal includes absolute paths so the agent can reference them via file-reading capabilities.

**Fire Dialog:** Reuses NewAgentDialog pattern — agent selection, skip-permissions, worktree (ON by default), auto-generated branch name (`task/keywords` from description).

**UI Integration:**
- Center panel has Tasks | Terminal tab switching (Tasks is default view)
- Terminal stays mounted via CSS `hidden` to preserve xterm.js state
- Firing a task keeps user on Task Board; clicking sidebar session switches to Terminal
- Task count badge on Tasks tab
- Draft input state (open/closed + text) persists across tab switches via Zustand store

**Cognitive design rationale:** Based on Risko & Gilbert's cognitive offloading research — externalizing working memory to the task board reduces cognitive load. Batched evaluation (Review column) supports 60–90 minute work cycles per the cognitive research. Preattentive color coding (green=running, amber=review, blue=done) for <200ms status recognition.

**Data model:** `tasks` table (SQLite) with FK to `repos` (CASCADE) and `sessions` (SET NULL). Status CHECK constraint enforces valid values. `images` column stores a JSON array of filenames (e.g., `["task-1-1710000000-0.png"]`). Image files stored on disk at `{repo_path}/.racc/images/`.

**Current status:** Fully implemented. Components: `TaskBoard/` (TaskBoard, TaskColumn, TaskCard, TaskInput, FireTaskDialog). Store: `taskStore.ts`. Backend: `task.rs` (create, list, update_status, update_description, update_images, delete, save_task_image, copy_file_to_task_images, delete_task_image, rename_task_image).

### 4. File Viewer & Command Palette (implemented)

A zero-footprint file viewer designed around cognitive science principles — no persistent file tree, no extra tabs. Files are viewed on demand and the UI disappears completely when closed.

**Two trigger mechanisms (all funnel through a single `openFile()` action):**
1. **Cmd+P command palette** — fuzzy file search powered by `nucleo-matcher`, respects `.gitignore` via `ignore` crate
2. **Terminal path click** — Cmd+Click on file paths detected in xterm.js terminal output (regex-based link provider)

**Full overlay viewer:**
- Shiki syntax highlighting (`github-dark-default` theme, VS Code-compatible TextMate grammars)
- Cmd+F in-file search with match count, navigation (Enter/Shift+Enter), and current match highlighting
- Ctrl+G jump-to-line
- Layered Esc dismiss (search → viewer → close)
- Click-to-highlight line
- 95% opacity overlay to avoid figure-ground interference while keeping the sidebar visible for preattentive status monitoring
- Bottom status strip showing branch, session status, and elapsed time

**Cognitive design rationale:** Progressive disclosure (Information Foraging Theory) — the developer sees only what they need, when they need it. The command palette provides "information scent" through fuzzy matching scores. The dual-mode design (inline preview vs. full overlay) respects Cowan's 4±1 working memory limit by not overwhelming with content.

**Current status:** Fully implemented. Components: `FileViewer.tsx`, `CommandPalette.tsx`. State: `fileViewerStore.ts`. Backend: `file.rs` (`read_file`, `search_files` commands).

---

## P1: Important, Deferred

These features significantly enhance the product but are not required for initial validation.

| Feature | Description | Dependency |
|---------|-------------|------------|
| **Codex Support** | Add Codex CLI as a supported agent | Agent adapter |
| **Docker Sandbox** | Opt-in container-based environment isolation | Docker integration |
| **Task Queue Enhancements** | Task dependencies, priority ordering, bulk operations, drag-and-drop | Task Board (implemented) |
| **Cross-Machine Session Management** | Connect to remote agent sessions via Tailscale, manage from one dashboard | Tailscale integration |

---

## P2: Future Vision

Lower priority — depends on ecosystem maturity and user feedback.

| Feature | Description | Blocker |
|---------|-------------|---------|
| **Spec-Driven Development** | Built-in requirements.md / tasks.md editor tied to agent execution | Workflow design needed |
| **Global Knowledge Base** | Cross-session CLAUDE.md management and sync | Multi-session maturity |

[Next: UI Design >](UI-Design.md)
