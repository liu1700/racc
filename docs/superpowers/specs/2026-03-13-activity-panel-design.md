# Activity Panel — Design Spec

## Problem

Racc manages multiple parallel AI agent sessions, but the developer can only see one session's terminal at a time. The sidebar shows a status dot per session, but nothing about *what* each session is actively doing. When 3–5 agents run simultaneously, the developer has no peripheral awareness of background session activity — violating the cognitive design principle that each session should be compressible into a single cognitive chunk (Cowan's 4±1 limit) with strong information scent (Pirolli & Card).

## Solution

A collapsible **Activity Panel** above the existing StatusBar that shows one compact horizontal bar per active session, each displaying a real-time action summary parsed from PTY output.

```
┌─────────────────────────────────────────────────────────────┐
│  Sidebar (15%)  │   Terminal (55%)   │   Right Panel (30%)  │
│                 │                    │                      │
│                 │                    │                      │
├─────────────────┴────────────────────┴──────────────────────┤
│  ▾ Activity                                                 │  ← NEW
│  🟢 claude-code (feat/auth)      Reading src/auth/login.ts  │
│  🟢 claude-code (fix/nav)        Running command: npm test   │
│  🔵 claude-code (refactor/db)    Completed (exit 0)    [fading] │
├─────────────────────────────────────────────────────────────┤
│  Sessions: 2 running · 1 completed │ Total Tokens: 45.2k   │  ← existing StatusBar
└─────────────────────────────────────────────────────────────┘
```

## Cognitive Design Grounding

This feature is designed according to the principles in `wiki/Cognitive-Design-Research.md`:

### Tier 1–2 Ambient Information (Calm Technology)

The Activity Panel operates at **Tier 1 (ambient) and Tier 2 (informational)** in the notification architecture. It provides peripheral awareness without demanding attention — the developer glances down to check status, never gets interrupted by it.

### Single Cognitive Chunk Per Session

Each bar compresses a session into one preattentive chunk: **status color + identifier + action text**. The developer holds N categorical items, not N sessions worth of terminal output. With 3–5 running sessions, this stays well within Cowan's 4±1 limit.

### Information Scent for Foraging Decisions

The action summary (`Reading src/auth/login.ts`) provides enough scent for the developer to decide: "Is this session worth investigating?" without switching to it. This reduces unnecessary attention switches (each costing 100–500ms plus attention residue).

### Preattentive Pop-out

Status color is the sole preattentive channel. A session that turns amber (waiting for approval) or red (error) will pop out automatically among green (running) peers — detectable in under 200ms regardless of how many bars exist.

### Mode Separation Support

The panel auto-shows during **monitoring mode** (sessions running) and auto-hides when there's nothing to monitor, supporting the deep-work/monitoring mode separation. The developer is never asked to passively watch it — it's there when they glance, invisible when not needed.

### Anti-Fatigue

- No animations beyond the existing status pulse (already validated as non-distracting at 2s cycle)
- Completed sessions fade out gently over 5s rather than disappearing abruptly (avoids startle/scan disruption)
- Maximum 5 visible bars before scrolling — prevents the panel from dominating the viewport
- Auto-collapse when empty removes visual noise entirely

## Architecture

### Component: PTY Output Parser

**File:** `src/services/ptyOutputParser.ts`

Subscribes to PTY output streams via `ptyManager.subscribe()` and extracts structured activity state through pattern matching.

**Per-session state:**

```typescript
interface SessionActivity {
  sessionId: number;
  action: string;        // "Reading", "Editing", "Running command", "Thinking", etc.
  detail: string | null; // file path, command summary, or null
  timestamp: number;     // Date.now() — for idle detection and fade timing
}
```

**Internal mechanics:**

- Maintains a ~100-line text buffer per session (decoded from `Uint8Array` via `TextDecoder`) for pattern matching context. Buffer size is a tunable constant (`PARSER_BUFFER_LINES = 100`).
- **ANSI escape stripping:** All decoded text is stripped of ANSI escape sequences before being added to the line buffer. Use a lightweight regex (`/\x1b\[[0-9;]*[a-zA-Z]/g`) to remove color codes, cursor movement, and other terminal control sequences. Without this, pattern matching will fail on real PTY output which is heavily decorated with escape codes.
- On each PTY data chunk: decode → strip ANSI → append to line buffer, run regex matchers, update activity if a pattern matches
- Buffer is a sliding window — old lines are dropped when the 100-line cap is hit

**Claude Code output patterns (initial set):**

| PTY output pattern | `action` | `detail` |
|---|---|---|
| `⏺ Read` followed by file path | `Reading` | extracted file path |
| `⏺ Edit` followed by file path | `Editing` | extracted file path |
| `⏺ Write` followed by file path | `Writing` | extracted file path |
| `⏺ Bash` or bash command context | `Running command` | command summary (truncated to ~40 chars) |
| `⏺ Search`, `⏺ Glob`, `⏺ Grep` | `Searching` | pattern or path |
| Thinking indicator / spinner characters | `Thinking` | `null` |
| Permission prompt (`Allow`, `Do you want to`) | `Waiting for approval` | `null` |
| `[Process exited with code X]` | `Completed` | `exit X` |
| No output for >10 seconds | `Idle` | `null` |

**Action-to-status-color mapping:**

Each action maps to a status dot color so the bar's visual priority is immediately clear via preattentive pop-out:

| Action | Dot color | Token |
|--------|-----------|-------|
| Reading, Editing, Writing, Running command, Searching, Thinking | Green | `status-running` |
| Waiting for approval | Amber | `status-waiting` |
| Idle | Dimmed green (green at 50% opacity) | `status-running/50` |
| Completed (exit 0) | Blue | `status-completed` |
| Completed (non-zero exit) | Red | `status-error` |

Note: This is the *activity-level* dot color displayed in the Activity Panel bar. It is independent of the session-level `SessionStatus` stored in the database. The session's DB status remains one of the four values (`Running | Completed | Disconnected | Error`); the activity dot provides finer-grained visual feedback based on parsed PTY output.

**Agent-specific parsing:**

The parser is selected based on the session's `agent` field. Initial implementation covers `claude-code` only. The architecture supports adding parsers for `aider`, `codex`, etc. via a simple registry pattern:

```typescript
type AgentParser = (lines: string[], latestChunk: string) => { action: string; detail: string | null } | null;

const parsers: Record<string, AgentParser> = {
  "claude-code": parseClaudeCodeOutput,
  // future: "aider": parseAiderOutput,
};
```

**Lifecycle:**

- `startTracking(sessionId: number, agent: string)` — called when a session enters Running state, **after `spawnPty()` has returned** (ensuring the PTY entry exists in the map). Subscribes to PTY output via `ptyManager.subscribe()`. If `subscribe()` returns `null` (PTY not yet registered — should not happen if call order is correct), retry once after a microtask (`queueMicrotask`) and log a warning if still null.
- `stopTracking(sessionId: number)` — called when a session stops. Unsubscribes the listener, clears the line buffer.
- The parser does NOT manage its own state store — it calls back into Zustand via a provided callback.

### Store: Session Activity State

**Location:** Extend `src/stores/sessionStore.ts`

New state fields:

```typescript
// State
sessionActivities: Record<number, SessionActivity>;  // sessionId → latest activity
activityPanelOpen: boolean;
activityPanelDismissed: boolean;  // user manually closed → suppress auto-open

// Actions
updateSessionActivity: (sessionId: number, activity: SessionActivity) => void;
removeSessionActivity: (sessionId: number) => void;
setActivityPanelOpen: (open: boolean) => void;
dismissActivityPanel: () => void;   // user-initiated close
```

**Auto-open/close logic:**

- On `updateSessionActivity`: if `activityPanelOpen === false` and `activityPanelDismissed === false` → set `activityPanelOpen = true`
- On `removeSessionActivity`: if no remaining activities → set `activityPanelOpen = false`
- On `dismissActivityPanel`: set both `activityPanelOpen = false` and `activityPanelDismissed = true`
- On `createSession`: reset `activityPanelDismissed = false` only when transitioning from zero running sessions to one (i.e., this is the first new session after all previous ones finished). If sessions are already running and the user dismissed the panel, respect that dismissal — they're likely in deep work mode.

**De-duplication:** `updateSessionActivity` compares `action + detail` with current value. If identical, skip the `set()` call entirely — do not trigger a Zustand re-render. The `timestamp` for idle detection is tracked internally in the parser (not in the store), since the store does not need to re-render on timestamp-only changes.

### Component: Activity Panel UI

**File:** `src/components/ActivityPanel/ActivityPanel.tsx`

**Layout:**

- Full-width bar positioned between the main content area and StatusBar in `App.tsx`
- Collapsible via CSS `max-height` + `opacity` transition (150ms, matching existing FileViewer transition timing)
- Maximum height: 5 bars × 28px = 140px, then vertical scroll with `overflow-y: auto`

**Each session bar:**

```
┌─────────────────────────────────────────────────────────────┐
│ ● claude-code (feat/auth)                Reading src/App.tsx │
│ ↑ status dot    ↑ agent + branch           ↑ action + detail │
└─────────────────────────────────────────────────────────────┘
```

- **Height:** 28px, `px-4 py-1`
- **Background:** `surface-1`, bar itself `surface-2`, hover `surface-3`
- **Left side:** Status color dot (6px, same `status-*` colors, `animate-status-pulse` for Running) + agent name + branch in `text-zinc-400`, truncated with ellipsis
- **Right side:** Action text in `text-zinc-400`, detail (file path / command) in `text-zinc-300` for slight emphasis. Truncated with ellipsis if too long.
- **Click:** Calls `setActiveSession(sessionId)` — same behavior as clicking a session in the sidebar
- **Cursor:** `cursor-pointer`
- **Active session highlight:** If the bar's session is the currently active session, show a subtle left border accent (`border-l-2 border-accent`)

**Panel header:**

- Small label "Activity" in `text-zinc-500 text-xs` at the left
- Collapse button (chevron icon `▾`/`▸`) at the right — toggles `activityPanelOpen`
- Top border: `border-t border-surface-3` (matches StatusBar's border style)

**Completed session fade-out:**

- When a session's action becomes `Completed`, the bar gets CSS class `animate-fade-out` (opacity 1→0 over 5s via CSS animation)
- After 5s, `removeSessionActivity(sessionId)` is called via `setTimeout`
- Cleanup: timeouts cleared on component unmount

**Collapse toggle on StatusBar:**

- Add a small clickable chevron (`▲`/`▼`) to the left end of StatusBar as an alternative toggle, so the user can re-open the panel after dismissing it

### Integration: App.tsx Layout Change

Current structure (simplified):

```tsx
<div className="flex h-screen flex-col">
  <div className="flex flex-1">
    <Sidebar />
    <main>
      <Terminal />
    </main>
    <RightPanel />
  </div>
  <StatusBar />
</div>
```

New structure:

```tsx
<div className="flex h-screen flex-col">
  <div className="flex flex-1 min-h-0 overflow-hidden">
    <Sidebar />
    <main>
      <Terminal />
    </main>
    <RightPanel />
  </div>
  <ActivityPanel />       {/* NEW — collapsible, between content and status bar */}
  <StatusBar />
</div>
```

The existing `overflow-hidden` is preserved (it prevents terminal/sidebar from expanding beyond the viewport). `min-h-0` is added so the flex row can shrink below its content size when the Activity Panel takes space.

### Integration: Session Lifecycle Hooks

Wire up the parser in `sessionStore.ts` actions:

- **`createSession`** → after PTY spawn succeeds, call `ptyOutputParser.startTracking(session.id, session.agent)` using the `Session` object returned by `invoke('create_session', ...)`
- **`stopSession`** → call `ptyOutputParser.stopTracking(sessionId)`, update activity to `Completed`
- **`reattachSession`** → call `startTracking` again (new PTY, fresh parser state)
- **`killPty` (in ptyManager)** → no change needed; `stopTracking` handles cleanup

### Tailwind Config Additions

New animation for the completed-session fade-out:

```typescript
// In tailwind.config.ts → theme.extend
animation: {
  "status-pulse": "...",            // existing
  "fade-out": "fade-out 5s ease-out forwards",  // NEW
},
keyframes: {
  "status-pulse": { ... },          // existing
  "fade-out": {                      // NEW
    "0%": { opacity: "1" },
    "80%": { opacity: "1" },         // hold visible for 4s
    "100%": { opacity: "0" },        // fade over last 1s
  },
},
```

The 80%→100% curve means the bar stays fully visible for 4 seconds, then fades over the final 1 second — giving the developer time to notice the completion without the bar lingering unnervingly.

## Visual Design Notes

Consistent with the existing design language:

- **No new colors** — reuses `surface-0/1/2/3`, `status-*`, `text-zinc-*`, `accent` tokens
- **No new fonts** — inherits the app's existing font stack (JetBrains Mono for monospace elements)
- **Motion restraint** — only two animations: the existing `status-pulse` (already validated) and the new `fade-out` (calm, non-attention-grabbing). No entrance animations, no bouncing, no sliding. The panel appears via a simple max-height transition.
- **Information density** — 28px per bar is compact enough to show 5 sessions without dominating the screen, but tall enough for comfortable click targets (exceeds minimum 24px touch target)
- **SEEV compliance** — the panel is within the bottom 20° visual angle from the terminal (the primary focus area), minimizing eye movement cost for status checks

## Scope Boundaries

**In scope:**
- PTY output parser for Claude Code agent
- Activity Panel UI component
- Store extensions for activity state
- Auto-open/close behavior
- Click-to-switch-session
- Completed session fade-out
- StatusBar collapse toggle

**Out of scope (future work):**
- Parsers for Aider, Codex, or other agents (architecture supports it, implementation deferred)
- Audio/sound notifications for status changes (belongs to Tier 3+ notification system, deferred per Wickens' Multiple Resource Theory — audio is valuable but requires its own design pass)
- Activity history / timeline view
- Customizable parser patterns
- Right-click context menu on bars
- Keyboard shortcut for panel toggle (e.g., `Cmd+Shift+A` — deferred to a holistic keyboard shortcut pass)

## File Manifest

| File | Action | Description |
|------|--------|-------------|
| `src/services/ptyOutputParser.ts` | Create | PTY output parser with Claude Code pattern matching |
| `src/components/ActivityPanel/ActivityPanel.tsx` | Create | Activity Panel UI component |
| `src/stores/sessionStore.ts` | Modify | Add activity state fields and actions |
| `src/App.tsx` | Modify | Insert ActivityPanel between content area and StatusBar |
| `src/components/Dashboard/StatusBar.tsx` | Modify | Add collapse toggle chevron |
| `tailwind.config.ts` | Modify | Add `fade-out` animation |
| `src/types/session.ts` | Modify | Add `SessionActivity` type |
