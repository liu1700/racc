# Activity Panel Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a collapsible Activity Panel above the StatusBar that shows real-time per-session action summaries parsed from PTY output.

**Architecture:** A PTY output parser service subscribes to each running session's output stream, strips ANSI codes, matches Claude Code output patterns via regex, and pushes structured `SessionActivity` updates into the Zustand store. A new `ActivityPanel` component renders one bar per active session with status dot, identifier, and action text. The panel auto-opens when sessions are running and auto-closes when empty.

**Tech Stack:** React 19, TypeScript, Zustand, Tailwind CSS, existing `ptyManager` subscriber API.

**Spec:** `docs/superpowers/specs/2026-03-13-activity-panel-design.md`

**No test framework is configured** in this project. Steps that would normally be TDD are implemented directly and verified manually via `bun run build` (TypeScript type-checking + Vite build).

---

## File Structure

| File | Action | Responsibility |
|------|--------|----------------|
| `src/types/session.ts` | Modify | Add `SessionActivity` interface |
| `src/services/ptyOutputParser.ts` | Create | PTY output parsing, ANSI stripping, Claude Code pattern matching, idle detection |
| `src/stores/sessionStore.ts` | Modify | Add activity state fields/actions, wire parser into lifecycle hooks |
| `src/components/ActivityPanel/ActivityPanel.tsx` | Create | Activity Panel UI — bars, header, collapse, fade-out |
| `src/App.tsx` | Modify | Insert `ActivityPanel` between content area and `StatusBar` |
| `src/components/Dashboard/StatusBar.tsx` | Modify | Add collapse toggle chevron |
| `tailwind.config.ts` | Modify | Add `fade-out` keyframes and animation |

---

## Chunk 1: Data Layer (Types + Parser Service + Store)

### Task 1: Add SessionActivity type

**Files:**
- Modify: `src/types/session.ts:19-24`

- [ ] **Step 1: Add the SessionActivity interface**

Add after the `Session` interface at the end of the file:

```typescript
export interface SessionActivity {
  sessionId: number;
  action: string;
  detail: string | null;
  timestamp: number;
}
```

- [ ] **Step 2: Verify build**

Run: `cd /home/devuser/racc && bun run build`
Expected: Build succeeds with no type errors.

- [ ] **Step 3: Commit**

```bash
git add src/types/session.ts
git commit -m "feat(types): add SessionActivity interface for activity panel"
```

---

### Task 2: Create PTY output parser service

**Files:**
- Create: `src/services/ptyOutputParser.ts`

This is the core parsing engine. It subscribes to PTY output, strips ANSI codes, matches Claude Code patterns, and calls a callback with activity updates.

- [ ] **Step 1: Create the parser module**

Create `src/services/ptyOutputParser.ts`:

```typescript
import { subscribe } from "./ptyManager";
import type { SessionActivity } from "../types/session";

const PARSER_BUFFER_LINES = 100;
const IDLE_TIMEOUT_MS = 10_000;

// Strip ANSI escape sequences from terminal output
function stripAnsi(str: string): string {
  return str.replace(/\x1b\[[0-9;]*[a-zA-Z]/g, "");
}

// --- Claude Code pattern matchers ---

// Tool use patterns: ⏺ Read, ⏺ Edit, ⏺ Write, ⏺ Bash, etc.
// The ⏺ character may appear with surrounding ANSI codes, so we match after stripping.
const TOOL_PATTERN = /[⏺●]\s*(Read|Edit|Write|Bash|Search|Glob|Grep|Agent)\b/;
const FILE_PATH_PATTERN = /(?:^|\s)((?:\/|\.\.?\/|src\/|tests?\/)\S+)/;
const BASH_CMD_PATTERN = /[⏺●]\s*Bash\b[^]*?(?:\$|>)\s*(.+)/;
const PERMISSION_PATTERN = /(?:Allow|Do you want to|Approve|Yes\/No|allow this)/i;
const EXIT_PATTERN = /\[Process exited with code (\d+)\]/;

type ActivityCallback = (sessionId: number, activity: SessionActivity) => void;

interface TrackedSession {
  sessionId: number;
  agent: string;
  lines: string[];
  unsubscribe: (() => void) | null;
  lastActivityTime: number;
  idleTimer: ReturnType<typeof setTimeout> | null;
  decoder: TextDecoder;
}

const tracked = new Map<number, TrackedSession>();
let onActivityUpdate: ActivityCallback | null = null;

/** Set the callback that receives activity updates. Call once at app init. */
export function setActivityCallback(cb: ActivityCallback): void {
  onActivityUpdate = cb;
}

function emitActivity(sessionId: number, action: string, detail: string | null): void {
  if (!onActivityUpdate) return;

  const entry = tracked.get(sessionId);
  if (entry) {
    entry.lastActivityTime = Date.now();

    // Reset idle timer (but not when emitting Idle itself to avoid infinite loop)
    if (entry.idleTimer) clearTimeout(entry.idleTimer);
    if (action !== "Idle") {
      entry.idleTimer = setTimeout(() => {
        emitActivity(sessionId, "Idle", null);
      }, IDLE_TIMEOUT_MS);
    }
  }

  onActivityUpdate(sessionId, {
    sessionId,
    action,
    detail,
    timestamp: Date.now(),
  });
}

function parseClaudeCodeOutput(lines: string[], latestChunk: string): { action: string; detail: string | null } | null {
  // Check latest chunk first for most recent activity

  // Permission prompt
  if (PERMISSION_PATTERN.test(latestChunk)) {
    return { action: "Waiting for approval", detail: null };
  }

  // Process exit
  const exitMatch = latestChunk.match(EXIT_PATTERN);
  if (exitMatch) {
    return { action: "Completed", detail: `exit ${exitMatch[1]}` };
  }

  // Tool use
  const toolMatch = latestChunk.match(TOOL_PATTERN);
  if (toolMatch) {
    const tool = toolMatch[1];

    switch (tool) {
      case "Read": {
        const fileMatch = latestChunk.match(FILE_PATH_PATTERN);
        return { action: "Reading", detail: fileMatch?.[1]?.slice(0, 60) ?? null };
      }
      case "Edit": {
        const fileMatch = latestChunk.match(FILE_PATH_PATTERN);
        return { action: "Editing", detail: fileMatch?.[1]?.slice(0, 60) ?? null };
      }
      case "Write": {
        const fileMatch = latestChunk.match(FILE_PATH_PATTERN);
        return { action: "Writing", detail: fileMatch?.[1]?.slice(0, 60) ?? null };
      }
      case "Bash": {
        const cmdMatch = latestChunk.match(BASH_CMD_PATTERN);
        const cmd = cmdMatch?.[1]?.trim().slice(0, 40) ?? null;
        return { action: "Running command", detail: cmd };
      }
      case "Search":
      case "Glob":
      case "Grep": {
        const pathMatch = latestChunk.match(FILE_PATH_PATTERN);
        return { action: "Searching", detail: pathMatch?.[1]?.slice(0, 60) ?? null };
      }
      case "Agent": {
        return { action: "Running agent", detail: null };
      }
    }
  }

  // Thinking — check for common spinner/thinking indicators
  // Claude Code shows a spinner or "Thinking..." text
  if (/thinking|\.{3,}$/i.test(latestChunk.trim())) {
    return { action: "Thinking", detail: null };
  }

  return null;
}

type AgentParser = (lines: string[], latestChunk: string) => { action: string; detail: string | null } | null;

const parsers: Record<string, AgentParser> = {
  "claude-code": parseClaudeCodeOutput,
};

function handlePtyData(sessionId: number, data: Uint8Array): void {
  const entry = tracked.get(sessionId);
  if (!entry) return;

  const decoded = stripAnsi(entry.decoder.decode(data, { stream: true }));
  if (!decoded.trim()) return;

  // Split into lines and add to buffer
  const newLines = decoded.split(/\r?\n/);
  entry.lines.push(...newLines);

  // Trim buffer to max size
  if (entry.lines.length > PARSER_BUFFER_LINES) {
    entry.lines.splice(0, entry.lines.length - PARSER_BUFFER_LINES);
  }

  // Run parser
  const parser = parsers[entry.agent];
  if (!parser) return;

  const result = parser(entry.lines, decoded);
  if (result) {
    emitActivity(sessionId, result.action, result.detail);
  }
}

/** Start tracking a session's PTY output. Call AFTER spawnPty() has returned. */
export function startTracking(sessionId: number, agent: string): void {
  // Clean up if already tracking
  stopTracking(sessionId);

  const entry: TrackedSession = {
    sessionId,
    agent,
    lines: [],
    unsubscribe: null,
    lastActivityTime: Date.now(),
    idleTimer: null,
    decoder: new TextDecoder(),
  };

  tracked.set(sessionId, entry);

  // Subscribe to PTY output
  const unsub = subscribe(sessionId, (data) => handlePtyData(sessionId, data));

  if (unsub) {
    entry.unsubscribe = unsub;
  } else {
    // PTY not yet registered — retry once after microtask
    console.warn(`[ptyOutputParser] subscribe returned null for session ${sessionId}, retrying...`);
    queueMicrotask(() => {
      const retryUnsub = subscribe(sessionId, (data) => handlePtyData(sessionId, data));
      if (retryUnsub) {
        entry.unsubscribe = retryUnsub;
      } else {
        console.warn(`[ptyOutputParser] subscribe still null for session ${sessionId}, giving up`);
        tracked.delete(sessionId);
      }
    });
  }

  // Emit initial activity (this also starts the idle timer via emitActivity)
  emitActivity(sessionId, "Starting", null);
}

/** Stop tracking a session. Cleans up listener and buffers. */
export function stopTracking(sessionId: number): void {
  const entry = tracked.get(sessionId);
  if (!entry) return;

  if (entry.unsubscribe) entry.unsubscribe();
  if (entry.idleTimer) clearTimeout(entry.idleTimer);
  tracked.delete(sessionId);
}
```

- [ ] **Step 2: Verify build**

Run: `cd /home/devuser/racc && bun run build`
Expected: Build succeeds. The parser is not yet wired up — just needs to compile.

- [ ] **Step 3: Commit**

```bash
git add src/services/ptyOutputParser.ts
git commit -m "feat(parser): add PTY output parser for session activity tracking"
```

---

### Task 3: Extend Zustand store with activity state and wire lifecycle

**Files:**
- Modify: `src/stores/sessionStore.ts:1-178`

- [ ] **Step 1: Add imports for the parser**

At the top of `src/stores/sessionStore.ts`, add imports:

```typescript
import { startTracking, stopTracking, setActivityCallback } from "../services/ptyOutputParser";
import type { SessionActivity } from "../types/session";
```

Add `SessionActivity` to the existing import from `../types/session` (merge it with the existing `Repo, Session, RepoWithSessions` import).

- [ ] **Step 2: Extend the SessionState interface**

Add these fields to the `SessionState` interface (after line 11, before `getActiveSession`):

```typescript
  sessionActivities: Record<number, SessionActivity>;
  activityPanelOpen: boolean;
  activityPanelDismissed: boolean;

  updateSessionActivity: (sessionId: number, activity: SessionActivity) => void;
  removeSessionActivity: (sessionId: number) => void;
  setActivityPanelOpen: (open: boolean) => void;
  dismissActivityPanel: () => void;
```

- [ ] **Step 3: Add initial state values**

In the `create<SessionState>` call, after `error: null,` (line 34), add:

```typescript
  sessionActivities: {},
  activityPanelOpen: false,
  activityPanelDismissed: false,
```

- [ ] **Step 4: Add activity actions**

After the `clearError` action (line 177), add these actions:

```typescript
  updateSessionActivity: (sessionId, activity) => {
    const current = get().sessionActivities[sessionId];
    // De-duplicate: skip set() if action + detail unchanged
    if (current && current.action === activity.action && current.detail === activity.detail) {
      return;
    }
    const { activityPanelOpen, activityPanelDismissed } = get();
    set({
      sessionActivities: { ...get().sessionActivities, [sessionId]: activity },
      // Auto-open panel if not user-dismissed
      ...(!activityPanelOpen && !activityPanelDismissed ? { activityPanelOpen: true } : {}),
    });
  },

  removeSessionActivity: (sessionId) => {
    const { [sessionId]: _, ...rest } = get().sessionActivities;
    const hasRemaining = Object.keys(rest).length > 0;
    set({
      sessionActivities: rest,
      // Auto-close when no remaining activities
      ...(!hasRemaining ? { activityPanelOpen: false } : {}),
    });
  },

  setActivityPanelOpen: (open) => set({ activityPanelOpen: open }),

  dismissActivityPanel: () =>
    set({ activityPanelOpen: false, activityPanelDismissed: true }),
```

- [ ] **Step 5: Wire setActivityCallback in initialize()**

In the `initialize` action, at the very beginning (before `set({ loading: true, ... })`), add:

```typescript
    // Wire up the PTY output parser callback
    setActivityCallback((sessionId, activity) => {
      get().updateSessionActivity(sessionId, activity);
    });
```

- [ ] **Step 6: Wire startTracking in createSession()**

In the `createSession` action, right after the `spawnPty(session.id, cwd, 80, 24, agentCmd);` call (line 111), add:

```typescript
      // Start tracking PTY output for activity panel
      startTracking(session.id, session.agent);
```

Also, add logic to reset `activityPanelDismissed` when going from zero running sessions to one. Right before `spawnPty(...)`, add:

```typescript
      // Reset panel dismissed state if this is the first running session
      const runningSessions = get().repos.flatMap((r) => r.sessions).filter((s) => s.status === "Running");
      if (runningSessions.length === 0) {
        set({ activityPanelDismissed: false });
      }
```

- [ ] **Step 7: Wire stopTracking in stopSession()**

In the `stopSession` action, **before** the `killPty(sessionId)` call (line 145), add:

```typescript
      stopTracking(sessionId);
      // Update activity to show completion before removing
      get().updateSessionActivity(sessionId, {
        sessionId,
        action: "Completed",
        detail: null,
        timestamp: Date.now(),
      });
```

- [ ] **Step 8: Wire startTracking in reattachSession()**

In the `reattachSession` action, right **before** the `spawnPty(...)` call (line 133), add the same dismissed-reset logic:

```typescript
      // Reset panel dismissed state if this is the first running session
      const runningSessions = get().repos.flatMap((r) => r.sessions).filter((s) => s.status === "Running");
      if (runningSessions.length === 0) {
        set({ activityPanelDismissed: false });
      }
```

Then right **after** the `spawnPty(...)` call, add:

```typescript
      startTracking(session.id, session.agent);
```

- [ ] **Step 9: Wire stopTracking in removeSession()**

In the `removeSession` action, **before** the `killPty(sessionId)` call (line 161), add:

```typescript
      stopTracking(sessionId);
      get().removeSessionActivity(sessionId);
```

- [ ] **Step 10: Verify build**

Run: `cd /home/devuser/racc && bun run build`
Expected: Build succeeds with no type errors.

- [ ] **Step 11: Commit**

```bash
git add src/stores/sessionStore.ts
git commit -m "feat(store): add session activity state and wire PTY parser lifecycle"
```

---

## Chunk 2: UI Layer (Tailwind + ActivityPanel + App Integration + StatusBar)

### Task 4: Add fade-out animation to Tailwind config

**Files:**
- Modify: `tailwind.config.ts:27-36`

- [ ] **Step 1: Add the fade-out keyframes and animation**

In `tailwind.config.ts`, extend the existing `animation` object (line 28) to add the fade-out entry:

```typescript
      animation: {
        "status-pulse":
          "status-pulse 2s cubic-bezier(0.4, 0, 0.6, 1) infinite",
        "fade-out": "fade-out 5s ease-out forwards",
      },
```

And extend the `keyframes` object (line 32) to add the fade-out keyframes:

```typescript
      keyframes: {
        "status-pulse": {
          "0%, 100%": { opacity: "1" },
          "50%": { opacity: "0.4" },
        },
        "fade-out": {
          "0%": { opacity: "1" },
          "80%": { opacity: "1" },
          "100%": { opacity: "0" },
        },
      },
```

- [ ] **Step 2: Verify build**

Run: `cd /home/devuser/racc && bun run build`
Expected: Build succeeds.

- [ ] **Step 3: Commit**

```bash
git add tailwind.config.ts
git commit -m "feat(tailwind): add fade-out animation for completed session bars"
```

---

### Task 5: Create ActivityPanel component

**Files:**
- Create: `src/components/ActivityPanel/ActivityPanel.tsx`

- [ ] **Step 1: Create the component directory**

Run: `mkdir -p /home/devuser/racc/src/components/ActivityPanel`

- [ ] **Step 2: Create the ActivityPanel component**

Create `src/components/ActivityPanel/ActivityPanel.tsx`:

```tsx
import { useEffect, useRef } from "react";
import { useSessionStore } from "../../stores/sessionStore";
import type { SessionActivity } from "../../types/session";

/** Map activity action to a status dot color class. */
function activityDotClass(activity: SessionActivity): string {
  switch (activity.action) {
    case "Waiting for approval":
      return "bg-status-waiting";
    case "Idle":
      return "bg-status-running/50";
    case "Completed":
      return activity.detail === "exit 0" ? "bg-status-completed" : "bg-status-error";
    default:
      return "bg-status-running";
  }
}

/** Whether this activity's dot should pulse. */
function shouldPulse(action: string): boolean {
  return action !== "Idle" && action !== "Completed" && action !== "Waiting for approval";
}

/** Look up the session's branch from the repos list. */
function useSessionBranch(sessionId: number): { agent: string; branch: string } {
  const repos = useSessionStore((s) => s.repos);
  for (const rws of repos) {
    const session = rws.sessions.find((s) => s.id === sessionId);
    if (session) {
      return { agent: session.agent, branch: session.branch ?? "main" };
    }
  }
  return { agent: "agent", branch: "main" };
}

function ActivityBar({
  activity,
  isActive,
  onSelect,
}: {
  activity: SessionActivity;
  isActive: boolean;
  onSelect: () => void;
}) {
  const { agent, branch } = useSessionBranch(activity.sessionId);
  const isCompleted = activity.action === "Completed";

  return (
    <button
      type="button"
      onClick={onSelect}
      className={`flex w-full cursor-pointer items-center justify-between px-4 py-1 text-xs transition-colors duration-150 ${
        isActive ? "border-l-2 border-accent bg-surface-2" : "border-l-2 border-transparent hover:bg-surface-3"
      } ${isCompleted ? "animate-fade-out" : ""}`}
      style={{ height: "28px" }}
    >
      {/* Left: status dot + agent + branch */}
      <span className="flex items-center gap-2 overflow-hidden">
        <span
          className={`h-1.5 w-1.5 flex-shrink-0 rounded-full ${activityDotClass(activity)} ${
            shouldPulse(activity.action) ? "animate-status-pulse" : ""
          }`}
        />
        <span className="truncate text-zinc-400">
          {agent} <span className="text-zinc-500">({branch})</span>
        </span>
      </span>

      {/* Right: action + detail */}
      <span className="ml-4 max-w-[50%] truncate text-right text-zinc-400">
        {activity.action}
        {activity.detail && (
          <span className="ml-1 text-zinc-300">{activity.detail}</span>
        )}
      </span>
    </button>
  );
}

export function ActivityPanel() {
  const activities = useSessionStore((s) => s.sessionActivities);
  const panelOpen = useSessionStore((s) => s.activityPanelOpen);
  const dismissPanel = useSessionStore((s) => s.dismissActivityPanel);
  const setOpen = useSessionStore((s) => s.setActivityPanelOpen);
  const setActiveSession = useSessionStore((s) => s.setActiveSession);
  const activeSessionId = useSessionStore((s) => s.activeSessionId);
  const removeSessionActivity = useSessionStore((s) => s.removeSessionActivity);

  // Track fade-out timeouts for cleanup
  const fadeTimers = useRef<Map<number, ReturnType<typeof setTimeout>>>(new Map());

  const activityList = Object.values(activities);

  // Clean up all fade timers on unmount only
  useEffect(() => {
    return () => {
      for (const [, timer] of fadeTimers.current) {
        clearTimeout(timer);
      }
    };
  }, []);

  // Set up fade-out timers for completed sessions
  // The .has() guard prevents duplicate timers even though activityList is a new ref each render
  useEffect(() => {
    for (const activity of activityList) {
      if (activity.action === "Completed" && !fadeTimers.current.has(activity.sessionId)) {
        const timer = setTimeout(() => {
          removeSessionActivity(activity.sessionId);
          fadeTimers.current.delete(activity.sessionId);
        }, 5000);
        fadeTimers.current.set(activity.sessionId, timer);
      }
    }
  }, [activityList, removeSessionActivity]);

  // Nothing to show
  if (activityList.length === 0 && !panelOpen) return null;

  return (
    <div
      className={`border-t border-surface-3 bg-surface-1 transition-all duration-150 ${
        panelOpen ? "max-h-40 opacity-100" : "max-h-0 opacity-0 overflow-hidden"
      }`}
    >
      {/* Header */}
      <div className="flex items-center justify-between px-4 py-0.5">
        <span className="text-xs font-medium uppercase tracking-wider text-zinc-500">
          Activity
        </span>
        <button
          type="button"
          onClick={() => setOpen(false)}
          className="text-zinc-500 hover:text-zinc-300 transition-colors text-xs px-1"
          title="Collapse activity panel"
        >
          ▾
        </button>
      </div>

      {/* Session bars */}
      <div className="max-h-[140px] overflow-y-auto">
        {activityList.map((activity) => (
          <ActivityBar
            key={activity.sessionId}
            activity={activity}
            isActive={activity.sessionId === activeSessionId}
            onSelect={() => setActiveSession(activity.sessionId)}
          />
        ))}
      </div>
    </div>
  );
}
```

- [ ] **Step 3: Verify build**

Run: `cd /home/devuser/racc && bun run build`
Expected: Build succeeds.

- [ ] **Step 4: Commit**

```bash
git add src/components/ActivityPanel/ActivityPanel.tsx
git commit -m "feat(ui): add ActivityPanel component with session bars and fade-out"
```

---

### Task 6: Integrate ActivityPanel into App.tsx

**Files:**
- Modify: `src/App.tsx:1-55`

- [ ] **Step 1: Add import**

At the top of `src/App.tsx`, add after the `StatusBar` import (line 5):

```typescript
import { ActivityPanel } from "./components/ActivityPanel/ActivityPanel";
```

- [ ] **Step 2: Add min-h-0 to main content div**

Change line 32 from:

```tsx
      <div className="flex flex-1 overflow-hidden">
```

to:

```tsx
      <div className="flex flex-1 min-h-0 overflow-hidden">
```

- [ ] **Step 3: Insert ActivityPanel between content and StatusBar**

After the closing `</div>` of the main content area (line 46) and before `<StatusBar />` (line 49), insert:

```tsx
      {/* Activity Panel — session activity monitor */}
      <ActivityPanel />
```

- [ ] **Step 4: Verify build**

Run: `cd /home/devuser/racc && bun run build`
Expected: Build succeeds.

- [ ] **Step 5: Commit**

```bash
git add src/App.tsx
git commit -m "feat(layout): integrate ActivityPanel into app layout"
```

---

### Task 7: Add collapse toggle to StatusBar

**Files:**
- Modify: `src/components/Dashboard/StatusBar.tsx:1-107`

- [ ] **Step 1: Add store imports**

At the top of `StatusBar.tsx`, the `useSessionStore` import already exists (line 3). We just need to access two more store fields. No new import lines needed.

- [ ] **Step 2: Add toggle state from store**

Inside the `StatusBar` component function, after the existing store selectors (lines 17-20), add:

```typescript
  const activityPanelOpen = useSessionStore((s) => s.activityPanelOpen);
  const setActivityPanelOpen = useSessionStore((s) => s.setActivityPanelOpen);
  const sessionActivities = useSessionStore((s) => s.sessionActivities);
  const hasActivities = Object.keys(sessionActivities).length > 0;
```

- [ ] **Step 3: Add the toggle chevron**

In the `<footer>` JSX, at the very beginning of the first `<div>` (the left side, line 76), add a chevron button before the "Sessions:" span:

```tsx
        {hasActivities && (
          <button
            type="button"
            onClick={() => setActivityPanelOpen(!activityPanelOpen)}
            className="mr-2 text-zinc-500 hover:text-zinc-300 transition-colors"
            title={activityPanelOpen ? "Hide activity panel" : "Show activity panel"}
          >
            {activityPanelOpen ? "▼" : "▲"}
          </button>
        )}
```

- [ ] **Step 4: Verify build**

Run: `cd /home/devuser/racc && bun run build`
Expected: Build succeeds.

- [ ] **Step 5: Commit**

```bash
git add src/components/Dashboard/StatusBar.tsx
git commit -m "feat(statusbar): add activity panel collapse toggle"
```

---

## Chunk 3: Manual Verification

### Task 8: End-to-end verification

This task verifies everything works together by running the app.

- [ ] **Step 1: Build check**

Run: `cd /home/devuser/racc && bun run build`
Expected: TypeScript type-checking passes. Vite production build succeeds.

- [ ] **Step 2: Manual smoke test (if Tauri environment available)**

Run: `cd /home/devuser/racc && bun tauri dev`

Test checklist:
1. Import a repo and create a session → Activity Panel should auto-open with "Starting" bar
2. Watch Claude Code run → bar should update with actions (Reading, Editing, etc.)
3. Click a session bar → terminal should switch to that session
4. Click the ▾ button on the panel → panel should collapse
5. Click the ▲ chevron on StatusBar → panel should re-open
6. Stop a session → bar should show "Completed" and fade out over 5 seconds
7. After all sessions complete and fade → panel should auto-collapse
8. Create a new session after dismissing → panel should auto-open again

- [ ] **Step 3: Final commit (if any fixes needed)**

```bash
git add -A
git commit -m "fix: address issues found during manual testing"
```

Note: Only run this step if Step 2 revealed issues that required code changes.
