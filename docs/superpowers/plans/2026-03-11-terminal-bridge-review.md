# Plan Review: Terminal Bridge (Issue #3)

**Reviewer:** Code Review Agent
**Plan:** `/docs/superpowers/plans/2026-03-11-terminal-bridge.md`
**Issue:** https://github.com/liu1700/otte/issues/3

---

## 1. Acceptance Criteria Coverage

| Criteria | Covered | Plan Location |
|----------|---------|---------------|
| "+ New" dialog with project/branch/agent | Yes | Chunk 2, Task 2 |
| tmux session created on submit | Yes | Chunk 1, Task 1 |
| Terminal shows live agent output | Yes | Chunk 3 (polling) + Chunk 4 (wire) |
| User can type into terminal, input reaches agent | Yes | Chunk 3, `onData` handler |
| Sidebar reflects session status | Partial | Chunk 5 adds auto-fetch, but no status *change* detection |
| Stop button kills session | Yes | Already exists in Sidebar; Chunk 5 improves error handling |

**Gap: Session status detection.** Issue scope item 3 asks to "detect session status changes (running -> waiting -> completed)." The plan's `list_sessions` Rust command always returns `SessionStatus::Running` and hardcodes `agent: "claude-code"`. Neither the plan nor the existing code parses tmux state to detect if the agent process exited. This is a missing piece -- even a basic check (e.g., `tmux list-panes -t <session> -F '#{pane_dead}'`) would satisfy the acceptance criteria better.

**Verdict:** All 5 hard acceptance criteria are addressed. The "detect status changes" scope item from section 3 is not covered but was likely considered optional polish.

---

## 2. File Path Correctness

All file paths in the plan match the existing codebase layout:

| Plan Path | Exists | Action |
|-----------|--------|--------|
| `src-tauri/src/commands/session.rs` | Yes | Modify |
| `src-tauri/src/commands/tmux.rs` | Yes | Modify |
| `src-tauri/src/lib.rs` | Yes | Modify |
| `src/components/Sidebar/Sidebar.tsx` | Yes | Modify |
| `src/components/Sidebar/NewSessionDialog.tsx` | No (new) | Create |
| `src/hooks/useTmuxBridge.ts` | No (new) | Create |
| `src/components/Terminal/Terminal.tsx` | Yes | Modify |
| `src/stores/sessionStore.ts` | Yes | Modify |
| `src/App.tsx` | Yes | Modify |

**Note:** The `src/hooks/` directory does not exist yet. The plan should include `mkdir -p src/hooks` or the implementer should know to create it. This is minor -- most editors create parent dirs automatically.

All import paths within the planned code are correct relative to the file structure.

---

## 3. Code Correctness Issues

### Critical

**(C1) `list_sessions` parsing is broken for the session name format.**
The existing `list_sessions` uses `splitn(3, '-')` on names like `otte-myproject-feat/new-feature`. This splits into `["otte", "myproject", "feat/new-feature"]` and maps index 1 to project, index 2 to branch. However, if the project name contains a hyphen (e.g., `my-app`), the split produces `["otte", "my", "app-branchname"]` -- corrupting both fields. The plan does not fix this function. A delimiter change (e.g., `__` instead of `-`) or a metadata file approach would be more robust.

**(C2) `capture_pane` polling with `terminal.reset()` causes severe flickering.**
The plan's `useTmuxBridge` calls `terminal.reset()` then `terminal.write(content)` every 150ms whenever content changes. `reset()` clears the entire screen, causing visible flicker. A better approach:
- Use `terminal.clear()` + `terminal.write()`, or
- Diff the output and write only incremental content, or
- Use `\x1b[2J\x1b[H` (clear + home) instead of full reset, which avoids destroying scroll state.

### Important

**(I1) `resize_pane` does not check tmux command success.**
In the planned `tmux.rs`, `resize_pane` fires `resize-window` but ignores the exit status. All other commands in the file check `output.status.success()` or at least propagate errors. This should be consistent.

**(I2) `create_session` does not set a working directory for `git worktree add`.**
The `git worktree add` command runs without setting `.current_dir()`. It will use whatever the Tauri process's cwd is (likely the app bundle location), not the user's project repository. The command needs `.current_dir(&project_repo_path)` or the user needs to provide the repo path. This is a functional bug -- worktree creation will fail or create worktrees from the wrong repo.

**(I3) Missing `useState` import in `Sidebar.tsx`.**
The plan instructs to add `import { useState } from "react"` but the current file does not import from React at all. The plan's instruction is correct, but it should be explicit that this is a new import line, not a modification of an existing one.

**(I4) `send_keys` with `-l` (literal) may not handle multi-byte/unicode correctly.**
tmux `send-keys -l` sends literal characters, but pasting multi-character strings rapidly can cause ordering issues. For the MVP polling approach this is acceptable, but worth noting for future improvement.

**(I5) No cleanup of git worktrees on `stop_session`.**
When a session is stopped, the tmux session is killed but the git worktree at `~/otte-worktrees/<project>/<branch>` remains on disk. Over time this accumulates stale worktrees. The plan should add worktree cleanup to `stop_session` or document this as a known limitation.

### Suggestions

**(S1) `capture_pane -e` flag for ANSI colors.**
The plan adds `-e` to capture escape sequences. This is good for color support, but xterm.js may double-interpret some sequences. If rendering looks garbled, removing `-e` and relying on the raw text may work better. Worth testing.

**(S2) Poll interval could be configurable from UI.**
150ms is reasonable, but for low-activity sessions a longer interval saves CPU. Consider adaptive polling (fast when recent changes detected, slow when idle).

**(S3) The `NewSessionDialog` lacks keyboard shortcut to close (Escape key).**
Adding an `onKeyDown` handler for Escape would improve UX.

**(S4) Consider adding `allowProposedApi: true` comment.**
The plan's Terminal.tsx sets `allowProposedApi: true` on the xterm instance. A comment explaining why this is needed (for `onData` or other proposed APIs) would help future maintainers.

---

## 4. Task Ordering and Dependencies

The dependency flow is correct:

```
Chunk 1 (Backend Fixes) -- no frontend deps, pure Rust
    |
Chunk 2 (NewSessionDialog) -- depends on Chunk 1 for `create_session` to work
    |
Chunk 3 (useTmuxBridge hook) -- depends on Chunk 1 for `send_keys`/`capture_pane`/`resize_pane`
    |
Chunk 4 (Wire Terminal) -- depends on Chunk 3 for the hook
    |
Chunk 5 (Session Store + App) -- depends on Chunks 2+4 for full integration
    |
Chunk 6 (Integration Test) -- depends on all above
```

Chunks 2 and 3 could technically be done in parallel since they don't depend on each other, but sequential is fine and safer. No circular dependencies detected.

---

## 5. Summary

**What is done well:**
- Correct identification of all existing code issues (relative worktree path, auto-Enter on send_keys)
- Clean separation into backend/frontend chunks with proper build verification steps
- Good keyboard input mapping in useTmuxBridge covering arrow keys, Ctrl sequences, etc.
- Proper Tauri 2.x patterns (`invoke` from `@tauri-apps/api/core`)
- All import paths are correct for the project structure

**Must fix before implementation:**
1. **(C1)** Session name parsing in `list_sessions` will break on hyphenated project names
2. **(C2)** `terminal.reset()` on every poll cycle will cause visible flickering
3. **(I2)** `git worktree add` needs a `current_dir` set to the source repo

**Should fix:**
4. **(I1)** `resize_pane` should check exit status
5. **(I5)** Add worktree cleanup on session stop (or document as known gap)

**Recommendation:** Address C1, C2, and I2 before starting implementation. The remaining items can be fixed during or after implementation without blocking.
