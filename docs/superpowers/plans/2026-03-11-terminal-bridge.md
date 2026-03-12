# Terminal Bridge Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Wire xterm.js to real tmux sessions so users can create, interact with, and stop agent sessions from the Racc UI.

**Architecture:** Frontend polls Rust backend for tmux output via `capture_pane` on a 150ms interval. User keystrokes go through `send_keys`. A "New Session" dialog triggers `create_session` which creates a worktree + tmux session + launches the agent. Session state is managed in a Zustand store.

**Tech Stack:** React 19, TypeScript, Zustand, xterm.js, Tauri 2.x IPC (`invoke`), Rust (`std::process::Command` wrapping tmux CLI)

---

## File Structure

| Action | Path | Responsibility |
|--------|------|----------------|
| Create | `src/components/Sidebar/NewSessionDialog.tsx` | Modal dialog for creating sessions |
| Modify | `src/components/Sidebar/Sidebar.tsx` | Wire "+ New" button to dialog |
| Modify | `src/components/Terminal/Terminal.tsx` | Replace placeholder with tmux bridge |
| Create | `src/hooks/useTmuxBridge.ts` | Poll capture_pane, forward send_keys |
| Modify | `src/stores/sessionStore.ts` | Add error handling, fetch on mount |
| Modify | `src-tauri/src/commands/session.rs` | Fix worktree path, add session existence check, improve error handling |
| Modify | `src-tauri/src/commands/tmux.rs` | Add `resize_pane` command, fix `send_keys` to not auto-append Enter |

---

## Chunk 1: Backend Fixes

### Task 1: Fix Rust session creation command

The current `create_session` has issues: hardcoded relative worktree path, always creates a new branch (fails if branch exists), `send_keys` always appends "Enter", session names use `-` delimiter which conflicts with hyphenated project/branch names, and git commands lack `current_dir`. Fix these before wiring the frontend.

**Key design decisions:**
- Session naming uses `::` delimiter: `racc::project::branch` (avoids conflict with `-` in names)
- `create_session` takes a `repo_path` param so git commands run in the correct directory
- `send_keys` split into literal text (`-l` flag) and special keys (Enter, C-c, etc.)

**Files:**
- Modify: `src-tauri/src/commands/session.rs`
- Modify: `src-tauri/src/commands/tmux.rs`
- Modify: `src-tauri/src/lib.rs`

- [ ] **Step 1: Fix `create_session` to use absolute worktree paths and handle existing branches**

In `src-tauri/src/commands/session.rs`, replace the entire `create_session` function:

```rust
#[tauri::command]
pub async fn create_session(
    repo_path: String,
    project: String,
    branch: String,
    agent: String,
) -> Result<Session, String> {
    let session_name = format!("racc::{}::{}", project, branch);

    // Check if tmux session already exists
    let check = Command::new("tmux")
        .args(["has-session", "-t", &session_name])
        .output()
        .map_err(|e| format!("Failed to check tmux session: {}", e))?;

    if check.status.success() {
        return Err(format!("Session '{}' already exists", session_name));
    }

    // Resolve absolute worktree path relative to home directory
    let home = std::env::var("HOME").map_err(|_| "HOME not set".to_string())?;
    let worktree_path = format!("{}/racc-worktrees/{}/{}", home, project, branch);

    // Create parent directory (but not the worktree dir itself — git needs it absent)
    let parent = std::path::Path::new(&worktree_path)
        .parent()
        .ok_or("Invalid worktree path")?;
    std::fs::create_dir_all(parent)
        .map_err(|e| format!("Failed to create worktree parent dir: {}", e))?;

    // Try to create worktree with new branch, fall back to existing branch
    let output = Command::new("git")
        .current_dir(&repo_path)
        .args(["worktree", "add", &worktree_path, "-b", &branch])
        .output()
        .map_err(|e| format!("Failed to create worktree: {}", e))?;

    if !output.status.success() {
        // Branch might already exist, try without -b
        let output2 = Command::new("git")
            .current_dir(&repo_path)
            .args(["worktree", "add", &worktree_path, &branch])
            .output()
            .map_err(|e| format!("Failed to create worktree: {}", e))?;

        if !output2.status.success() {
            let stderr = String::from_utf8_lossy(&output2.stderr);
            return Err(format!("Failed to create worktree: {}", stderr));
        }
    }

    // Create tmux session with working directory
    let tmux_output = Command::new("tmux")
        .args([
            "new-session",
            "-d",
            "-s",
            &session_name,
            "-c",
            &worktree_path,
            "-x",
            "200",
            "-y",
            "50",
        ])
        .output()
        .map_err(|e| format!("Failed to create tmux session: {}", e))?;

    if !tmux_output.status.success() {
        let stderr = String::from_utf8_lossy(&tmux_output.stderr);
        return Err(format!("Failed to create tmux session: {}", stderr));
    }

    // Start agent in tmux session
    let agent_cmd = match agent.as_str() {
        "claude-code" => "claude",
        "aider" => "aider",
        "codex" => "codex",
        "shell" => "bash",
        _ => return Err(format!("Unknown agent: {}", agent)),
    };

    Command::new("tmux")
        .args(["send-keys", "-t", &session_name, agent_cmd, "Enter"])
        .output()
        .map_err(|e| format!("Failed to start agent: {}", e))?;

    Ok(Session {
        id: session_name.clone(),
        name: session_name,
        project,
        branch,
        agent,
        status: SessionStatus::Running,
        worktree_path,
    })
}
```

- [ ] **Step 2: Fix `list_sessions` to use `::` delimiter**

In `src-tauri/src/commands/session.rs`, replace the `list_sessions` function:

```rust
#[tauri::command]
pub async fn list_sessions() -> Result<Vec<Session>, String> {
    let output = Command::new("tmux")
        .args(["list-sessions", "-F", "#{session_name}"])
        .output()
        .map_err(|e| format!("Failed to list tmux sessions: {}", e))?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let sessions: Vec<Session> = stdout
        .lines()
        .filter(|line| line.starts_with("racc::"))
        .filter_map(|name| {
            // Format: racc::project::branch
            let rest = name.strip_prefix("racc::")?;
            let (project, branch) = rest.split_once("::")?;
            Some(Session {
                id: name.to_string(),
                name: name.to_string(),
                project: project.to_string(),
                branch: branch.to_string(),
                agent: "claude-code".to_string(),
                status: SessionStatus::Running,
                worktree_path: String::new(),
            })
        })
        .collect();

    Ok(sessions)
}
```

- [ ] **Step 3: Fix `send_keys` to not auto-append Enter, and add `resize_pane`**

In `src-tauri/src/commands/tmux.rs`, replace the entire file:

```rust
use std::process::Command;

#[tauri::command]
pub async fn send_keys(session_id: String, keys: String) -> Result<(), String> {
    // Send raw keys without auto-appending Enter.
    // The frontend is responsible for sending Enter when needed.
    Command::new("tmux")
        .args(["send-keys", "-t", &session_id, "-l", &keys])
        .output()
        .map_err(|e| format!("Failed to send keys: {}", e))?;

    Ok(())
}

#[tauri::command]
pub async fn send_special_key(session_id: String, key: String) -> Result<(), String> {
    // Send special keys like Enter, C-c, Escape, etc.
    Command::new("tmux")
        .args(["send-keys", "-t", &session_id, &key])
        .output()
        .map_err(|e| format!("Failed to send special key: {}", e))?;

    Ok(())
}

#[tauri::command]
pub async fn capture_pane(session_id: String) -> Result<String, String> {
    let output = Command::new("tmux")
        .args([
            "capture-pane",
            "-t",
            &session_id,
            "-p",    // print to stdout
            "-e",    // include escape sequences (ANSI colors)
            "-S",
            "-1000", // scroll buffer: last 1000 lines
        ])
        .output()
        .map_err(|e| format!("Failed to capture pane: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("capture-pane failed: {}", stderr));
    }

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

#[tauri::command]
pub async fn resize_pane(session_id: String, cols: u32, rows: u32) -> Result<(), String> {
    Command::new("tmux")
        .args([
            "resize-window",
            "-t",
            &session_id,
            "-x",
            &cols.to_string(),
            "-y",
            &rows.to_string(),
        ])
        .output()
        .map_err(|e| format!("Failed to resize pane: {}", e))?;

    Ok(())
}
```

- [ ] **Step 4: Register new commands in lib.rs**

In `src-tauri/src/lib.rs`, add `send_special_key` and `resize_pane` to the handler:

```rust
mod commands;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .invoke_handler(tauri::generate_handler![
            commands::session::create_session,
            commands::session::list_sessions,
            commands::session::stop_session,
            commands::tmux::send_keys,
            commands::tmux::send_special_key,
            commands::tmux::capture_pane,
            commands::tmux::resize_pane,
            commands::git::create_worktree,
            commands::git::delete_worktree,
            commands::git::get_diff,
            commands::cost::get_usage,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
```

- [ ] **Step 5: Verify Rust compiles**

Run: `cd src-tauri && cargo check`
Expected: `Finished` with no errors.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/commands/session.rs src-tauri/src/commands/tmux.rs src-tauri/src/lib.rs
git commit -m "fix: improve session creation and tmux commands for real usage"
```

---

## Chunk 2: New Session Dialog

### Task 2: Create the New Session dialog component

**Files:**
- Create: `src/components/Sidebar/NewSessionDialog.tsx`
- Modify: `src/components/Sidebar/Sidebar.tsx`

- [ ] **Step 1: Create NewSessionDialog component**

Create `src/components/Sidebar/NewSessionDialog.tsx`:

```tsx
import { useState } from "react";
import { useSessionStore } from "../../stores/sessionStore";

interface Props {
  open: boolean;
  onClose: () => void;
}

const AGENTS = [
  { id: "claude-code", label: "Claude Code" },
  { id: "aider", label: "Aider" },
  { id: "codex", label: "Codex" },
  { id: "shell", label: "Shell (bash)" },
];

export function NewSessionDialog({ open, onClose }: Props) {
  const [repoPath, setRepoPath] = useState("");
  const [project, setProject] = useState("");
  const [branch, setBranch] = useState("");
  const [agent, setAgent] = useState("claude-code");
  const [error, setError] = useState<string | null>(null);
  const [creating, setCreating] = useState(false);
  const createSession = useSessionStore((s) => s.createSession);

  if (!open) return null;

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!repoPath.trim() || !project.trim() || !branch.trim()) return;

    setCreating(true);
    setError(null);
    try {
      await createSession(repoPath.trim(), project.trim(), branch.trim(), agent);
      setProject("");
      setBranch("");
      setAgent("claude-code");
      onClose();
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setCreating(false);
    }
  };

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/60">
      <form
        onSubmit={handleSubmit}
        className="w-96 rounded-lg border border-surface-3 bg-surface-1 p-6 shadow-2xl"
      >
        <h2 className="mb-4 text-sm font-semibold text-zinc-200">
          New Session
        </h2>

        <label className="mb-3 block">
          <span className="mb-1 block text-xs text-zinc-400">
            Repository path
          </span>
          <input
            type="text"
            value={repoPath}
            onChange={(e) => setRepoPath(e.target.value)}
            placeholder="/Users/you/projects/my-repo"
            autoFocus
            className="w-full rounded border border-surface-3 bg-surface-2 px-3 py-1.5 text-sm text-white placeholder-zinc-600 outline-none focus:border-accent"
          />
        </label>

        <label className="mb-3 block">
          <span className="mb-1 block text-xs text-zinc-400">
            Project name
          </span>
          <input
            type="text"
            value={project}
            onChange={(e) => setProject(e.target.value)}
            placeholder="my-app"
            className="w-full rounded border border-surface-3 bg-surface-2 px-3 py-1.5 text-sm text-white placeholder-zinc-600 outline-none focus:border-accent"
          />
        </label>

        <label className="mb-3 block">
          <span className="mb-1 block text-xs text-zinc-400">Branch</span>
          <input
            type="text"
            value={branch}
            onChange={(e) => setBranch(e.target.value)}
            placeholder="feat/new-feature"
            className="w-full rounded border border-surface-3 bg-surface-2 px-3 py-1.5 text-sm text-white placeholder-zinc-600 outline-none focus:border-accent"
          />
        </label>

        <label className="mb-4 block">
          <span className="mb-1 block text-xs text-zinc-400">Agent</span>
          <select
            value={agent}
            onChange={(e) => setAgent(e.target.value)}
            className="w-full rounded border border-surface-3 bg-surface-2 px-3 py-1.5 text-sm text-white outline-none focus:border-accent"
          >
            {AGENTS.map((a) => (
              <option key={a.id} value={a.id}>
                {a.label}
              </option>
            ))}
          </select>
        </label>

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
            disabled={creating || !repoPath.trim() || !project.trim() || !branch.trim()}
            className="rounded bg-accent px-3 py-1.5 text-xs font-medium text-white hover:bg-accent-hover disabled:opacity-50"
          >
            {creating ? "Creating..." : "Create"}
          </button>
        </div>
      </form>
    </div>
  );
}
```

- [ ] **Step 2: Wire dialog to Sidebar "+ New" button**

In `src/components/Sidebar/Sidebar.tsx`, add state and render the dialog:

Add import at top:
```tsx
import { useState } from "react";
import { NewSessionDialog } from "./NewSessionDialog";
```

Inside `Sidebar()` component, add state:
```tsx
const [dialogOpen, setDialogOpen] = useState(false);
```

Replace the `+ New` button:
```tsx
<button
  onClick={() => setDialogOpen(true)}
  className="rounded bg-accent px-2 py-1 text-xs font-medium text-white hover:bg-accent-hover"
>
  + New
</button>
```

Add dialog before closing `</aside>`:
```tsx
<NewSessionDialog open={dialogOpen} onClose={() => setDialogOpen(false)} />
```

- [ ] **Step 3: Verify frontend builds**

Run: `bun run build`
Expected: Build succeeds with no errors.

- [ ] **Step 4: Commit**

```bash
git add src/components/Sidebar/NewSessionDialog.tsx src/components/Sidebar/Sidebar.tsx
git commit -m "feat: add New Session dialog with project/branch/agent inputs"
```

---

## Chunk 3: tmux Bridge Hook

### Task 3: Create the `useTmuxBridge` hook

This is the core piece — a React hook that polls `capture_pane` and forwards keystrokes via `send_keys`.

**Files:**
- Create: `src/hooks/useTmuxBridge.ts`

- [ ] **Step 1: Create the bridge hook**

Create `src/hooks/useTmuxBridge.ts`:

```ts
import { useEffect, useRef, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { Terminal } from "@xterm/xterm";

interface UseTmuxBridgeOptions {
  sessionId: string | null;
  terminal: Terminal | null;
  pollIntervalMs?: number;
}

export function useTmuxBridge({
  sessionId,
  terminal,
  pollIntervalMs = 150,
}: UseTmuxBridgeOptions) {
  const lastContentRef = useRef<string>("");
  const pollRef = useRef<ReturnType<typeof setInterval> | null>(null);

  // Poll capture_pane and write new content to xterm
  const poll = useCallback(async () => {
    if (!sessionId || !terminal) return;

    try {
      const content = await invoke<string>("capture_pane", {
        sessionId,
      });

      // Only update if content changed
      if (content !== lastContentRef.current) {
        lastContentRef.current = content;
        // Move cursor to home position and clear screen, then write new content.
        // Using ANSI sequences instead of terminal.reset() to avoid flickering.
        terminal.write("\x1b[H\x1b[2J");
        terminal.write(content);
      }
    } catch {
      // Session might have ended — stop polling silently
    }
  }, [sessionId, terminal]);

  // Start/stop polling when session or terminal changes
  useEffect(() => {
    if (!sessionId || !terminal) {
      if (pollRef.current) {
        clearInterval(pollRef.current);
        pollRef.current = null;
      }
      return;
    }

    // Initial fetch
    poll();

    // Start polling
    pollRef.current = setInterval(poll, pollIntervalMs);

    return () => {
      if (pollRef.current) {
        clearInterval(pollRef.current);
        pollRef.current = null;
      }
    };
  }, [sessionId, terminal, poll, pollIntervalMs]);

  // Forward keyboard input to tmux
  useEffect(() => {
    if (!sessionId || !terminal) return;

    const disposable = terminal.onData(async (data: string) => {
      try {
        if (data === "\r") {
          // Enter key
          await invoke("send_special_key", { sessionId, key: "Enter" });
        } else if (data === "\x03") {
          // Ctrl+C
          await invoke("send_special_key", { sessionId, key: "C-c" });
        } else if (data === "\x04") {
          // Ctrl+D
          await invoke("send_special_key", { sessionId, key: "C-d" });
        } else if (data === "\x1a") {
          // Ctrl+Z
          await invoke("send_special_key", { sessionId, key: "C-z" });
        } else if (data === "\x1b") {
          // Escape
          await invoke("send_special_key", { sessionId, key: "Escape" });
        } else if (data === "\x7f" || data === "\b") {
          // Backspace
          await invoke("send_special_key", { sessionId, key: "BSpace" });
        } else if (data === "\t") {
          // Tab
          await invoke("send_special_key", { sessionId, key: "Tab" });
        } else if (data.startsWith("\x1b[")) {
          // Arrow keys and other escape sequences
          const arrowMap: Record<string, string> = {
            "\x1b[A": "Up",
            "\x1b[B": "Down",
            "\x1b[C": "Right",
            "\x1b[D": "Left",
            "\x1b[H": "Home",
            "\x1b[F": "End",
            "\x1b[3~": "DC", // Delete
          };
          const mapped = arrowMap[data];
          if (mapped) {
            await invoke("send_special_key", { sessionId, key: mapped });
          }
        } else {
          // Regular text input
          await invoke("send_keys", { sessionId, keys: data });
        }
      } catch {
        // Session might have ended
      }
    });

    return () => disposable.dispose();
  }, [sessionId, terminal]);

  // Sync terminal size to tmux pane
  useEffect(() => {
    if (!sessionId || !terminal) return;

    const syncSize = () => {
      invoke("resize_pane", {
        sessionId,
        cols: terminal.cols,
        rows: terminal.rows,
      }).catch(() => {});
    };

    // Sync on mount
    syncSize();

    const disposable = terminal.onResize(syncSize);
    return () => disposable.dispose();
  }, [sessionId, terminal]);
}
```

- [ ] **Step 2: Verify frontend builds**

Run: `bun run build`
Expected: Build succeeds.

- [ ] **Step 3: Commit**

```bash
git add src/hooks/useTmuxBridge.ts
git commit -m "feat: add useTmuxBridge hook for tmux polling and input forwarding"
```

---

## Chunk 4: Wire Terminal Component

### Task 4: Rewrite Terminal to use the tmux bridge

**Files:**
- Modify: `src/components/Terminal/Terminal.tsx`

- [ ] **Step 1: Rewrite Terminal.tsx to use useTmuxBridge**

Replace the entire file:

```tsx
import { useEffect, useRef, useState, useCallback } from "react";
import { useSessionStore } from "../../stores/sessionStore";
import { useTmuxBridge } from "../../hooks/useTmuxBridge";
import type { Terminal as XTermType } from "@xterm/xterm";

export function Terminal() {
  const terminalRef = useRef<HTMLDivElement>(null);
  const [term, setTerm] = useState<XTermType | null>(null);
  const activeSessionId = useSessionStore((s) => s.activeSessionId);

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

      // Cleanup closure captures
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

  // Reset terminal content when switching sessions
  const prevSessionRef = useRef<string | null>(null);
  useEffect(() => {
    if (activeSessionId !== prevSessionRef.current && term) {
      term.reset();
      prevSessionRef.current = activeSessionId;
    }
  }, [activeSessionId, term]);

  // Wire up the tmux bridge
  useTmuxBridge({
    sessionId: activeSessionId,
    terminal: term,
  });

  // Focus terminal on click
  const handleClick = useCallback(() => {
    term?.focus();
  }, [term]);

  if (!activeSessionId) {
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

- [ ] **Step 2: Verify frontend builds**

Run: `bun run build`
Expected: Build succeeds.

- [ ] **Step 3: Commit**

```bash
git add src/components/Terminal/Terminal.tsx
git commit -m "feat: wire Terminal component to tmux via useTmuxBridge"
```

---

## Chunk 5: Session Lifecycle & Auto-Fetch

### Task 5: Improve session store with auto-fetch and error handling

**Files:**
- Modify: `src/stores/sessionStore.ts`
- Modify: `src/App.tsx`

- [ ] **Step 1: Update sessionStore with error state and auto-refresh**

Replace `src/stores/sessionStore.ts`:

```ts
import { create } from "zustand";
import { invoke } from "@tauri-apps/api/core";
import type { Session } from "../types/session";

interface SessionState {
  sessions: Session[];
  activeSessionId: string | null;
  loading: boolean;
  error: string | null;

  setActiveSession: (id: string) => void;
  fetchSessions: () => Promise<void>;
  createSession: (
    repoPath: string,
    project: string,
    branch: string,
    agent: string,
  ) => Promise<void>;
  stopSession: (id: string) => Promise<void>;
  clearError: () => void;
}

export const useSessionStore = create<SessionState>((set, get) => ({
  sessions: [],
  activeSessionId: null,
  loading: false,
  error: null,

  setActiveSession: (id) => set({ activeSessionId: id }),

  clearError: () => set({ error: null }),

  fetchSessions: async () => {
    try {
      const sessions = await invoke<Session[]>("list_sessions");
      const state = get();
      // If active session no longer exists, deselect it
      const activeStillExists = sessions.some(
        (s) => s.id === state.activeSessionId,
      );
      set({
        sessions,
        loading: false,
        activeSessionId: activeStillExists ? state.activeSessionId : null,
      });
    } catch {
      // tmux might not be running — that's OK, just show empty
      set({ sessions: [], loading: false });
    }
  },

  createSession: async (repoPath, project, branch, agent) => {
    set({ error: null });
    const session = await invoke<Session>("create_session", {
      repoPath,
      project,
      branch,
      agent,
    });
    set((state) => ({
      sessions: [...state.sessions, session],
      activeSessionId: session.id,
    }));
  },

  stopSession: async (id) => {
    try {
      await invoke("stop_session", { sessionId: id });
    } catch {
      // Session might already be dead — that's fine
    }
    set((state) => ({
      sessions: state.sessions.filter((s) => s.id !== id),
      activeSessionId:
        state.activeSessionId === id ? null : state.activeSessionId,
    }));
  },
}));
```

- [ ] **Step 2: Add auto-fetch on app mount in App.tsx**

In `src/App.tsx`, add session fetching on mount:

```tsx
import { useEffect } from "react";
import { Sidebar } from "./components/Sidebar/Sidebar";
import { Terminal } from "./components/Terminal/Terminal";
import { ActivityLog } from "./components/ActivityLog/ActivityLog";
import { CostTracker } from "./components/CostTracker/CostTracker";
import { StatusBar } from "./components/Dashboard/StatusBar";
import { useSessionStore } from "./stores/sessionStore";

function App() {
  const fetchSessions = useSessionStore((s) => s.fetchSessions);

  // Fetch existing sessions on mount and periodically refresh
  useEffect(() => {
    fetchSessions();
    const interval = setInterval(fetchSessions, 5000);
    return () => clearInterval(interval);
  }, [fetchSessions]);

  return (
    <div className="flex h-screen flex-col bg-surface-0">
      {/* Main Content */}
      <div className="flex flex-1 overflow-hidden">
        {/* Left Sidebar — Session List (~15%) */}
        <Sidebar />

        {/* Center — Agent Terminal (~55%) */}
        <main className="flex flex-1 flex-col border-x border-surface-3">
          <Terminal />
        </main>

        {/* Right Panel — Activity + Cost (~30%) */}
        <aside className="flex w-80 flex-col overflow-hidden">
          <CostTracker />
          <ActivityLog />
        </aside>
      </div>

      {/* Global Status Bar */}
      <StatusBar />
    </div>
  );
}

export default App;
```

- [ ] **Step 3: Verify frontend builds**

Run: `bun run build`
Expected: Build succeeds.

- [ ] **Step 4: Commit**

```bash
git add src/stores/sessionStore.ts src/App.tsx
git commit -m "feat: add session auto-fetch and improved error handling"
```

---

## Chunk 6: Integration Test

### Task 6: End-to-end smoke test

This task is manual verification that the full loop works.

- [ ] **Step 1: Verify full build compiles**

Run: `bun run build && cd src-tauri && cargo check`
Expected: Both succeed.

- [ ] **Step 2: Launch the app**

Run: `bun tauri dev`

- [ ] **Step 3: Test the full flow**

1. App opens with empty session list and "No active session" placeholder
2. Click "+ New" — dialog appears
3. Enter project: `test`, branch: `demo`, agent: `Shell (bash)` — click Create
4. Sidebar shows new session with green "Running" dot
5. Terminal area shows a bash prompt from the tmux session
6. Type `ls` + Enter in the terminal — output appears
7. Type `echo hello` + Enter — "hello" appears
8. Click "Stop" on the session in sidebar — session disappears
9. Terminal returns to "No active session" placeholder

- [ ] **Step 4: Final commit with any fixes**

```bash
git add -A
git commit -m "feat: terminal bridge complete — sessions, tmux polling, input forwarding"
```
