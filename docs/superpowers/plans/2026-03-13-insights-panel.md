# Insights Panel Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the right-side assistant chat panel with an actionable Insights feed that detects patterns across sessions and surfaces one-click suggestions.

**Architecture:** Hybrid frontend/backend — frontend captures PTY events and runs real-time rules (file conflicts, cost spikes, permission repeats); Rust backend persists events in SQLite and runs batch analysis (repeated prompts, startup patterns, similar sessions); LLM generates suggestion text only. Timeline-style UI with inline-expanding cards.

**Tech Stack:** React 19 + Zustand (frontend), Rust + SQLite + strsim crate (backend), Tauri IPC + events

**Spec:** `docs/superpowers/specs/2026-03-13-insights-panel-design.md`

---

## Chunk 1: Data Layer (Rust)

### Task 1: Database Migration v4

**Files:**
- Modify: `src-tauri/src/commands/db.rs:109` (add migration after v3 block)

- [ ] **Step 1: Add migration v4 to db.rs**

After the `if version < 3` block (line 109), add:

```rust
    if version < 4 {
        conn.execute_batch(
            "
        BEGIN;

        CREATE TABLE IF NOT EXISTS session_events (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            session_id INTEGER NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
            event_type TEXT NOT NULL,
            payload TEXT NOT NULL,
            created_at INTEGER NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_events_session ON session_events(session_id);
        CREATE INDEX IF NOT EXISTS idx_events_type ON session_events(event_type);

        CREATE TABLE IF NOT EXISTS insights (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            insight_type TEXT NOT NULL,
            severity TEXT NOT NULL,
            title TEXT NOT NULL,
            summary TEXT NOT NULL,
            detail_json TEXT NOT NULL,
            fingerprint TEXT NOT NULL,
            status TEXT NOT NULL DEFAULT 'active',
            created_at INTEGER NOT NULL,
            resolved_at INTEGER
        );
        CREATE UNIQUE INDEX IF NOT EXISTS idx_insights_fingerprint
            ON insights(fingerprint) WHERE status = 'active';

        PRAGMA user_version = 4;

        COMMIT;
        ",
        )
        .map_err(|e| format!("Migration v4 failed: {e}"))?;
    }
```

- [ ] **Step 2: Verify compilation**

Run: `cd src-tauri && cargo check`
Expected: compiles without errors

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/commands/db.rs
git commit -m "feat(db): add migration v4 — session_events and insights tables"
```

---

### Task 2: Insights Rust Command Module

**Files:**
- Create: `src-tauri/src/commands/insights.rs`
- Modify: `src-tauri/src/commands/mod.rs:7` (add module)
- Modify: `src-tauri/src/lib.rs:44-70` (register commands)

- [ ] **Step 1: Create insights.rs with structs and commands**

```rust
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::sync::Mutex;
use tauri::State;

#[derive(Debug, Serialize, Deserialize)]
pub struct SessionEvent {
    pub session_id: i64,
    pub event_type: String,
    pub payload: String, // JSON string
    pub created_at: i64, // Unix timestamp ms
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Insight {
    pub id: i64,
    pub insight_type: String,
    pub severity: String,
    pub title: String,
    pub summary: String,
    pub detail_json: String,
    pub fingerprint: String,
    pub status: String,
    pub created_at: i64,
    pub resolved_at: Option<i64>,
}

#[tauri::command]
pub async fn record_session_events(
    db: State<'_, Mutex<Connection>>,
    events: Vec<SessionEvent>,
) -> Result<(), String> {
    let conn = db.lock().map_err(|e| format!("DB lock error: {e}"))?;
    for event in &events {
        conn.execute(
            "INSERT INTO session_events (session_id, event_type, payload, created_at) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![event.session_id, event.event_type, event.payload, event.created_at],
        )
        .map_err(|e| format!("Failed to insert event: {e}"))?;
    }
    Ok(())
}

#[tauri::command]
pub async fn get_insights(
    db: State<'_, Mutex<Connection>>,
    status: Option<String>,
) -> Result<Vec<Insight>, String> {
    let conn = db.lock().map_err(|e| format!("DB lock error: {e}"))?;
    let status_filter = status.as_deref().unwrap_or("active");
    let mut stmt = conn
        .prepare(
            "SELECT id, insight_type, severity, title, summary, detail_json, fingerprint, status, created_at, resolved_at
             FROM insights WHERE status = ?1 ORDER BY created_at DESC",
        )
        .map_err(|e| format!("Query error: {e}"))?;

    let rows = stmt
        .query_map(rusqlite::params![status_filter], |row| {
            Ok(Insight {
                id: row.get(0)?,
                insight_type: row.get(1)?,
                severity: row.get(2)?,
                title: row.get(3)?,
                summary: row.get(4)?,
                detail_json: row.get(5)?,
                fingerprint: row.get(6)?,
                status: row.get(7)?,
                created_at: row.get(8)?,
                resolved_at: row.get(9)?,
            })
        })
        .map_err(|e| format!("Query error: {e}"))?;

    let mut insights = Vec::new();
    for row in rows {
        insights.push(row.map_err(|e| format!("Row error: {e}"))?);
    }
    Ok(insights)
}

#[tauri::command]
pub async fn update_insight_status(
    db: State<'_, Mutex<Connection>>,
    id: i64,
    status: String,
) -> Result<(), String> {
    let conn = db.lock().map_err(|e| format!("DB lock error: {e}"))?;
    let resolved_at: Option<i64> = if status == "applied" || status == "dismissed" || status == "expired" {
        Some(chrono::Utc::now().timestamp_millis())
    } else {
        None
    };
    conn.execute(
        "UPDATE insights SET status = ?1, resolved_at = ?2 WHERE id = ?3",
        rusqlite::params![status, resolved_at, id],
    )
    .map_err(|e| format!("Update error: {e}"))?;
    Ok(())
}

#[tauri::command]
pub async fn save_insight(
    db: State<'_, Mutex<Connection>>,
    insight_type: String,
    severity: String,
    title: String,
    summary: String,
    detail_json: String,
    fingerprint: String,
) -> Result<Option<i64>, String> {
    let conn = db.lock().map_err(|e| format!("DB lock error: {e}"))?;

    // Check for existing active insight with same fingerprint
    let existing: Option<i64> = conn
        .query_row(
            "SELECT id FROM insights WHERE fingerprint = ?1 AND status = 'active'",
            rusqlite::params![fingerprint],
            |row| row.get(0),
        )
        .ok();

    if existing.is_some() {
        return Ok(None); // Duplicate, skip
    }

    let now = chrono::Utc::now().timestamp_millis();
    conn.execute(
        "INSERT INTO insights (insight_type, severity, title, summary, detail_json, fingerprint, status, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'active', ?7)",
        rusqlite::params![insight_type, severity, title, summary, detail_json, fingerprint, now],
    )
    .map_err(|e| format!("Insert error: {e}"))?;

    let id = conn.last_insert_rowid();
    Ok(Some(id))
}

#[tauri::command]
pub async fn get_session_events(
    db: State<'_, Mutex<Connection>>,
    event_type: Option<String>,
    since: Option<i64>,
) -> Result<Vec<SessionEvent>, String> {
    let conn = db.lock().map_err(|e| format!("DB lock error: {e}"))?;

    let (sql, params): (String, Vec<Box<dyn rusqlite::types::ToSql>>) = match (&event_type, &since) {
        (Some(et), Some(s)) => (
            "SELECT session_id, event_type, payload, created_at FROM session_events WHERE event_type = ?1 AND created_at >= ?2 ORDER BY created_at DESC LIMIT 500".into(),
            vec![Box::new(et.clone()), Box::new(*s)],
        ),
        (Some(et), None) => (
            "SELECT session_id, event_type, payload, created_at FROM session_events WHERE event_type = ?1 ORDER BY created_at DESC LIMIT 500".into(),
            vec![Box::new(et.clone())],
        ),
        (None, Some(s)) => (
            "SELECT session_id, event_type, payload, created_at FROM session_events WHERE created_at >= ?1 ORDER BY created_at DESC LIMIT 500".into(),
            vec![Box::new(*s)],
        ),
        (None, None) => (
            "SELECT session_id, event_type, payload, created_at FROM session_events ORDER BY created_at DESC LIMIT 500".into(),
            vec![],
        ),
    };

    let mut stmt = conn.prepare(&sql).map_err(|e| format!("Query error: {e}"))?;
    let param_refs: Vec<&dyn rusqlite::types::ToSql> = params.iter().map(|p| p.as_ref()).collect();
    let rows = stmt
        .query_map(param_refs.as_slice(), |row| {
            Ok(SessionEvent {
                session_id: row.get(0)?,
                event_type: row.get(1)?,
                payload: row.get(2)?,
                created_at: row.get(3)?,
            })
        })
        .map_err(|e| format!("Query error: {e}"))?;

    let mut events = Vec::new();
    for row in rows {
        events.push(row.map_err(|e| format!("Row error: {e}"))?);
    }
    Ok(events)
}

#[tauri::command]
pub async fn append_to_file(path: String, content: String) -> Result<(), String> {
    use std::fs::OpenOptions;
    use std::io::Write;

    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .map_err(|e| format!("Failed to open {path}: {e}"))?;

    // Add newline separator before content if file is non-empty
    let metadata = std::fs::metadata(&path).map_err(|e| format!("Failed to stat {path}: {e}"))?;
    if metadata.len() > 0 {
        writeln!(file).map_err(|e| format!("Write error: {e}"))?;
    }
    write!(file, "{content}").map_err(|e| format!("Write error: {e}"))?;

    Ok(())
}
```

- [ ] **Step 2: Register module in mod.rs**

Add to `src-tauri/src/commands/mod.rs`:
```rust
pub mod insights;
```

- [ ] **Step 3: Register commands in lib.rs**

Add to the `invoke_handler` in `src-tauri/src/lib.rs` (after line 69, before the closing `]`):
```rust
            commands::insights::record_session_events,
            commands::insights::get_insights,
            commands::insights::update_insight_status,
            commands::insights::save_insight,
            commands::insights::get_session_events,
            commands::insights::append_to_file,
```

- [ ] **Step 4: Verify compilation**

Run: `cd src-tauri && cargo check`
Expected: compiles without errors

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/commands/insights.rs src-tauri/src/commands/mod.rs src-tauri/src/lib.rs
git commit -m "feat(insights): add Rust commands for event recording and insight management"
```

---

## Chunk 2: Frontend Types + Event Capture

### Task 3: Frontend Type Definitions

**Files:**
- Create: `src/types/insights.ts`

- [ ] **Step 1: Create types file**

```typescript
export type InsightType =
  | "repeated_prompt"
  | "startup_pattern"
  | "repeated_permission"
  | "cost_anomaly"
  | "file_conflict"
  | "similar_sessions";

export type InsightSeverity = "warning" | "alert" | "suggestion";

export type InsightStatus = "active" | "applied" | "dismissed" | "expired";

export type SessionEventType =
  | "user_input"
  | "permission_request"
  | "file_operation"
  | "cost_update"
  | "session_meta";

export interface SessionEvent {
  sessionId: number;
  eventType: SessionEventType;
  payload: Record<string, unknown>;
  createdAt: number; // Unix timestamp ms
}

export interface Insight {
  id: number;
  insight_type: InsightType;
  severity: InsightSeverity;
  title: string;
  summary: string;
  detail_json: string; // JSON string — parsed by UI as needed
  fingerprint: string;
  status: InsightStatus;
  created_at: number;
  resolved_at: number | null;
}

// Parsed detail structures per insight type
export interface RepeatedPromptDetail {
  matches: Array<{
    sessionId: number;
    branch: string | null;
    text: string;
    timestamp: number;
  }>;
  suggestedEntry?: string; // LLM-generated CLAUDE.md entry
}

export interface FileConflictDetail {
  filePath: string;
  sessions: Array<{
    sessionId: number;
    branch: string | null;
    operation: string;
    timestamp: number;
  }>;
}

export interface CostAnomalyDetail {
  sessionId: number;
  currentCost: number;
  averageCost: number;
  windowMinutes: number;
}

export interface RepeatedPermissionDetail {
  sessionId: number;
  permissionType: string;
  count: number;
}

export interface StartupPatternDetail {
  commands: string[];
  sessions: Array<{ sessionId: number; branch: string | null }>;
  suggestedEntry?: string;
}

export interface SimilarSessionsDetail {
  sessionA: { id: number; branch: string | null };
  sessionB: { id: number; branch: string | null };
  similarity: number;
  sharedFiles: string[];
}
```

- [ ] **Step 2: Verify TypeScript compiles**

Run: `bun run build`
Expected: no type errors

- [ ] **Step 3: Commit**

```bash
git add src/types/insights.ts
git commit -m "feat(insights): add TypeScript type definitions"
```

---

### Task 4: Extend ptyOutputParser for User Prompt Extraction

**Files:**
- Modify: `src/services/ptyOutputParser.ts`

- [ ] **Step 1: Add prompt extraction pattern and callback**

Add after line 20 (after `EXIT_PATTERN`):
```typescript
// Claude Code prompt markers — user input follows these
// Only match the Unicode prompt character ❯ (U+276F) to avoid false positives from > in diffs/markdown
const PROMPT_MARKER_PATTERN = /^❯\s+(.+)/;
const HUMAN_TURN_PATTERN = /^\s*Human:\s*$/;
```

Add a new callback type and variable after line 35 (`let onActivityUpdate`):
```typescript
type PromptCallback = (sessionId: number, text: string, position: number) => void;
let onPromptDetected: PromptCallback | null = null;

/** Set the callback that receives user prompt detections. */
export function setPromptCallback(cb: PromptCallback): void {
  onPromptDetected = cb;
}
```

Add prompt counter to `TrackedSession` interface (after line 31, `decoder`):
```typescript
  promptCount: number;
  inHumanTurn: boolean;
  lineBuffer: string;
```

Update the `startTracking` function's `entry` object to include the new fields:
```typescript
    promptCount: 0,
    inHumanTurn: false,
    lineBuffer: "",
```

- [ ] **Step 2: Add prompt detection logic to handlePtyData**

In `handlePtyData`, after the tool parser runs (after line 153 `emitActivity(sessionId, result.action, result.detail);`), add prompt detection:

```typescript
  // Detect user prompts from PTY output
  if (onPromptDetected) {
    for (const line of newLines) {
      const trimmed = line.trim();
      if (!trimmed) continue;

      // Check for human turn delimiter
      if (HUMAN_TURN_PATTERN.test(trimmed)) {
        entry.inHumanTurn = true;
        entry.lineBuffer = "";
        continue;
      }

      // Check for prompt marker (❯ or >)
      const promptMatch = trimmed.match(PROMPT_MARKER_PATTERN);
      if (promptMatch) {
        const text = promptMatch[1].trim();
        if (text.length > 5) { // Ignore very short inputs (likely just commands like "y")
          entry.promptCount++;
          onPromptDetected(sessionId, text, entry.promptCount);
        }
        entry.inHumanTurn = false;
        entry.lineBuffer = "";
      }
    }
  }
```

- [ ] **Step 3: Verify compilation**

Run: `bun run build`
Expected: no type errors

- [ ] **Step 4: Commit**

```bash
git add src/services/ptyOutputParser.ts
git commit -m "feat(parser): extract user prompts from PTY output"
```

---

### Task 5: Event Capture Service

**Files:**
- Create: `src/services/eventCapture.ts`

- [ ] **Step 1: Create eventCapture.ts**

```typescript
import { invoke } from "@tauri-apps/api/core";
import { setPromptCallback } from "./ptyOutputParser";
import type { SessionEvent, SessionEventType } from "../types/insights";

const FLUSH_INTERVAL_MS = 30_000;
const MAX_BUFFER_SIZE = 200;

let eventBuffer: Array<{
  session_id: number;
  event_type: string;
  payload: string;
  created_at: number;
}> = [];

let flushTimer: ReturnType<typeof setInterval> | null = null;

// Callbacks for real-time rule evaluation (set by insightsStore)
type EventListener = (event: SessionEvent) => void;
const listeners: EventListener[] = [];

export function addEventListener(cb: EventListener): () => void {
  listeners.push(cb);
  return () => {
    const idx = listeners.indexOf(cb);
    if (idx >= 0) listeners.splice(idx, 1);
  };
}

function emit(event: SessionEvent): void {
  // Notify real-time listeners
  for (const cb of listeners) {
    cb(event);
  }

  // Buffer for batch flush to Rust/SQLite
  eventBuffer.push({
    session_id: event.sessionId,
    event_type: event.eventType,
    payload: JSON.stringify(event.payload),
    created_at: event.createdAt,
  });

  if (eventBuffer.length >= MAX_BUFFER_SIZE) {
    flushEvents();
  }
}

async function flushEvents(): Promise<void> {
  if (eventBuffer.length === 0) return;

  const batch = eventBuffer.splice(0);
  try {
    await invoke("record_session_events", { events: batch });
  } catch (e) {
    console.error("[eventCapture] flush failed:", e);
    // Re-queue failed events (prepend to preserve order)
    eventBuffer.unshift(...batch);
  }
}

// --- Public API ---

export function recordEvent(
  sessionId: number,
  eventType: SessionEventType,
  payload: Record<string, unknown>,
): void {
  emit({
    sessionId,
    eventType,
    payload,
    createdAt: Date.now(),
  });
}

export function initEventCapture(): void {
  // Set up prompt detection callback
  setPromptCallback((sessionId, text, position) => {
    recordEvent(sessionId, "user_input", { text, position });
  });

  // Start periodic flush
  if (flushTimer) clearInterval(flushTimer);
  flushTimer = setInterval(flushEvents, FLUSH_INTERVAL_MS);

  // Flush on page unload
  window.addEventListener("beforeunload", () => {
    flushEvents();
  });
}

export function stopEventCapture(): void {
  if (flushTimer) {
    clearInterval(flushTimer);
    flushTimer = null;
  }
  flushEvents();
}
```

- [ ] **Step 2: Verify compilation**

Run: `bun run build`
Expected: no type errors

- [ ] **Step 3: Commit**

```bash
git add src/services/eventCapture.ts
git commit -m "feat(events): add event capture service with batched SQLite flush"
```

---

## Chunk 3: Insights Store + Real-time Rules

### Task 6: Insights Zustand Store

**Files:**
- Create: `src/stores/insightsStore.ts`

- [ ] **Step 1: Create the store**

```typescript
import { create } from "zustand";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { addEventListener, recordEvent } from "../services/eventCapture";
import type {
  Insight,
  SessionEvent,
  InsightType,
  InsightSeverity,
  FileConflictDetail,
  CostAnomalyDetail,
  RepeatedPermissionDetail,
} from "../types/insights";

interface InsightsState {
  insights: Insight[];
  expandedId: number | null;
  loading: boolean;

  // Real-time rule state — intentionally mutable Maps, not reactive.
  // These are internal tracking state, not rendered directly.
  _fileMap: Map<string, Set<number>>; // filePath → sessionIds
  _permissionCounts: Map<number, Map<string, number>>; // sessionId → permType → count
  _costHistory: Map<number, number[]>; // sessionId → rolling cost deltas
  _initialized: boolean;

  // Actions
  initialize: () => Promise<void>;
  loadInsights: () => Promise<void>;
  toggleExpand: (id: number) => void;
  dismissInsight: (id: number) => Promise<void>;
  applyInsight: (id: number) => Promise<void>;

  // Internal
  _addInsight: (insight: Insight) => void;
  _handleEvent: (event: SessionEvent) => void;
}

export const useInsightsStore = create<InsightsState>((set, get) => ({
  insights: [],
  expandedId: null,
  loading: false,

  _fileMap: new Map(),
  _permissionCounts: new Map(),
  _costHistory: new Map(),
  _initialized: false,

  initialize: async () => {
    // Guard against duplicate initialization (e.g. component remount)
    if (get()._initialized) return;
    set({ _initialized: true });

    // Load existing insights from SQLite
    await get().loadInsights();

    // Subscribe to real-time events from eventCapture
    addEventListener((event) => get()._handleEvent(event));

    // Listen for batch analysis results from Rust
    listen<Insight>("insight-detected", (e) => {
      get()._addInsight(e.payload);
    });

    // Set up batch analysis polling (every 5 minutes)
    // Silently catches errors — run_batch_analysis may not exist until Chunk 4 is deployed
    setInterval(() => {
      invoke("run_batch_analysis").catch(() => {});
    }, 5 * 60 * 1000);
  },

  loadInsights: async () => {
    set({ loading: true });
    try {
      const insights = await invoke<Insight[]>("get_insights", { status: "active" });
      set({ insights, loading: false });
    } catch (e) {
      console.error("[insights] load failed:", e);
      set({ loading: false });
    }
  },

  toggleExpand: (id) => {
    set((s) => ({ expandedId: s.expandedId === id ? null : id }));
  },

  dismissInsight: async (id) => {
    try {
      await invoke("update_insight_status", { id, status: "dismissed" });
      set((s) => ({
        insights: s.insights.filter((i) => i.id !== id),
        expandedId: s.expandedId === id ? null : s.expandedId,
      }));
    } catch (e) {
      console.error("[insights] dismiss failed:", e);
    }
  },

  applyInsight: async (id) => {
    try {
      await invoke("update_insight_status", { id, status: "applied" });
      set((s) => ({
        insights: s.insights.filter((i) => i.id !== id),
        expandedId: s.expandedId === id ? null : s.expandedId,
      }));
    } catch (e) {
      console.error("[insights] apply failed:", e);
    }
  },

  _addInsight: (insight) => {
    set((s) => {
      // Avoid duplicates by fingerprint
      if (s.insights.some((i) => i.fingerprint === insight.fingerprint)) return s;
      return { insights: [insight, ...s.insights] };
    });
  },

  _handleEvent: (event) => {
    const state = get();

    switch (event.eventType) {
      case "file_operation": {
        const { operation, filePath } = event.payload as { operation: string; filePath: string };
        if (operation !== "edit" && operation !== "write") break;

        const fileMap = state._fileMap;
        if (!fileMap.has(filePath)) fileMap.set(filePath, new Set());
        fileMap.get(filePath)!.add(event.sessionId);

        if (fileMap.get(filePath)!.size > 1) {
          const sessions = Array.from(fileMap.get(filePath)!);
          const fingerprint = `file_conflict:${filePath}:${sessions.sort().join(",")}`;

          // Check if already reported
          if (state.insights.some((i) => i.fingerprint === fingerprint)) break;

          const detail: FileConflictDetail = {
            filePath,
            sessions: sessions.map((sid) => ({
              sessionId: sid,
              branch: null,
              operation,
              timestamp: event.createdAt,
            })),
          };

          invoke<number | null>("save_insight", {
            insightType: "file_conflict",
            severity: "alert",
            title: `File conflict: ${filePath.split("/").pop()}`,
            summary: `Modified in ${sessions.length} sessions`,
            detailJson: JSON.stringify(detail),
            fingerprint,
          }).then((id) => {
            if (id != null) {
              get()._addInsight({
                id,
                insight_type: "file_conflict",
                severity: "alert",
                title: `File conflict: ${filePath.split("/").pop()}`,
                summary: `Modified in ${sessions.length} sessions`,
                detail_json: JSON.stringify(detail),
                fingerprint,
                status: "active",
                created_at: Date.now(),
                resolved_at: null,
              });
            }
          });
        }
        break;
      }

      case "cost_update": {
        const { estimatedCostUsd } = event.payload as { estimatedCostUsd: number };
        const history = state._costHistory;
        if (!history.has(event.sessionId)) history.set(event.sessionId, []);
        const costs = history.get(event.sessionId)!;
        costs.push(estimatedCostUsd);

        // Keep last 10 entries
        if (costs.length > 10) costs.shift();

        if (costs.length >= 3) {
          const avg = costs.slice(0, -1).reduce((a, b) => a + b, 0) / (costs.length - 1);
          const current = costs[costs.length - 1];

          if (current > avg * 3 && current > 0.5) {
            const fingerprint = `cost_anomaly:${event.sessionId}:${Math.floor(Date.now() / 600_000)}`; // 10-min window
            if (state.insights.some((i) => i.fingerprint === fingerprint)) break;

            const detail: CostAnomalyDetail = {
              sessionId: event.sessionId,
              currentCost: current,
              averageCost: avg,
              windowMinutes: 10,
            };

            invoke<number | null>("save_insight", {
              insightType: "cost_anomaly",
              severity: "alert",
              title: `Cost spike: session ${event.sessionId}`,
              summary: `$${current.toFixed(2)} in last interval (avg $${avg.toFixed(2)})`,
              detailJson: JSON.stringify(detail),
              fingerprint,
            }).then((id) => {
              if (id != null) {
                get()._addInsight({
                  id,
                  insight_type: "cost_anomaly",
                  severity: "alert",
                  title: `Cost spike: session ${event.sessionId}`,
                  summary: `$${current.toFixed(2)} in last interval (avg $${avg.toFixed(2)})`,
                  detail_json: JSON.stringify(detail),
                  fingerprint,
                  status: "active",
                  created_at: Date.now(),
                  resolved_at: null,
                });
              }
            });
          }
        }
        break;
      }

      case "permission_request": {
        const { permissionType } = event.payload as { permissionType: string };
        const permMap = state._permissionCounts;
        if (!permMap.has(event.sessionId)) permMap.set(event.sessionId, new Map());
        const sessionPerms = permMap.get(event.sessionId)!;
        const count = (sessionPerms.get(permissionType) || 0) + 1;
        sessionPerms.set(permissionType, count);

        if (count === 3) {
          const fingerprint = `repeated_perm:${event.sessionId}:${permissionType}`;
          if (state.insights.some((i) => i.fingerprint === fingerprint)) break;

          const detail: RepeatedPermissionDetail = {
            sessionId: event.sessionId,
            permissionType,
            count,
          };

          invoke<number | null>("save_insight", {
            insightType: "repeated_permission",
            severity: "warning",
            title: "Repeated permission requests",
            summary: `"${permissionType}" requested ${count} times`,
            detailJson: JSON.stringify(detail),
            fingerprint,
          }).then((id) => {
            if (id != null) {
              get()._addInsight({
                id,
                insight_type: "repeated_permission",
                severity: "warning",
                title: "Repeated permission requests",
                summary: `"${permissionType}" requested ${count} times`,
                detail_json: JSON.stringify(detail),
                fingerprint,
                status: "active",
                created_at: Date.now(),
                resolved_at: null,
              });
            }
          });
        }
        break;
      }
    }
  },
}));
```

- [ ] **Step 2: Verify compilation**

Run: `bun run build`
Expected: no type errors

- [ ] **Step 3: Commit**

```bash
git add src/stores/insightsStore.ts
git commit -m "feat(insights): add Zustand store with real-time detection rules"
```

---

## Chunk 4: Rust Batch Analysis Engine

### Task 7: Add strsim Dependency

**Files:**
- Modify: `src-tauri/Cargo.toml:27`

- [ ] **Step 1: Add strsim to dependencies**

Add after `nucleo-matcher = "0.3"` in `src-tauri/Cargo.toml`:
```toml
strsim = "0.11"
```

- [ ] **Step 2: Verify**

Run: `cd src-tauri && cargo check`
Expected: downloads strsim, compiles

- [ ] **Step 3: Commit**

```bash
git add src-tauri/Cargo.toml
git commit -m "chore: add strsim crate for string similarity"
```

---

### Task 8: Batch Analysis Implementation

**Files:**
- Modify: `src-tauri/src/commands/insights.rs` (add analysis functions)

- [ ] **Step 1: Add batch analysis command to insights.rs**

Add at the end of `insights.rs`:

```rust
use strsim::normalized_levenshtein;
use std::collections::{HashMap, HashSet};

#[derive(Debug, Serialize)]
struct DetectedInsight {
    insight_type: String,
    severity: String,
    title: String,
    summary: String,
    detail_json: String,
    fingerprint: String,
}

fn detect_repeated_prompts(conn: &Connection) -> Vec<DetectedInsight> {
    let seven_days_ago = chrono::Utc::now().timestamp_millis() - 7 * 24 * 60 * 60 * 1000;
    let mut stmt = match conn.prepare(
        "SELECT session_id, payload, created_at FROM session_events
         WHERE event_type = 'user_input' AND created_at >= ?1
         ORDER BY created_at DESC LIMIT 500",
    ) {
        Ok(s) => s,
        Err(_) => return vec![],
    };

    let events: Vec<(i64, String, i64)> = stmt
        .query_map(rusqlite::params![seven_days_ago], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?))
        })
        .ok()
        .map(|rows| rows.filter_map(|r| r.ok()).collect())
        .unwrap_or_default();

    // Parse payloads to get text
    let inputs: Vec<(i64, String, i64)> = events
        .into_iter()
        .filter_map(|(sid, payload, ts)| {
            let parsed: serde_json::Value = serde_json::from_str(&payload).ok()?;
            let text = parsed.get("text")?.as_str()?.to_lowercase().trim().to_string();
            if text.len() < 10 { return None; } // Skip very short inputs
            Some((sid, text, ts))
        })
        .collect();

    // Cluster by similarity
    let mut clusters: Vec<Vec<usize>> = vec![];
    let mut assigned = HashSet::new();

    for i in 0..inputs.len() {
        if assigned.contains(&i) { continue; }
        let mut cluster = vec![i];
        assigned.insert(i);

        for j in (i + 1)..inputs.len() {
            if assigned.contains(&j) { continue; }
            let sim = normalized_levenshtein(&inputs[i].1, &inputs[j].1);
            if sim >= 0.7 {
                cluster.push(j);
                assigned.insert(j);
            }
        }
        clusters.push(cluster);
    }

    let mut results = vec![];
    for cluster in clusters {
        // Need entries from ≥3 distinct sessions
        let session_ids: HashSet<i64> = cluster.iter().map(|&idx| inputs[idx].0).collect();
        if session_ids.len() < 3 { continue; }

        let mut sorted_sids: Vec<i64> = session_ids.into_iter().collect();
        sorted_sids.sort();
        let representative_text = &inputs[cluster[0]].1;
        let fingerprint = format!("repeated_prompt:{}:{}", sorted_sids.iter().map(|s| s.to_string()).collect::<Vec<_>>().join(","), &representative_text[..representative_text.len().min(50)]);

        // Check if already exists
        let existing: bool = conn
            .query_row(
                "SELECT COUNT(*) FROM insights WHERE fingerprint = ?1 AND status = 'active'",
                rusqlite::params![fingerprint],
                |row| row.get::<_, i64>(0),
            )
            .unwrap_or(0) > 0;
        if existing { continue; }

        let matches: Vec<serde_json::Value> = cluster.iter().map(|&idx| {
            serde_json::json!({
                "sessionId": inputs[idx].0,
                "text": inputs[idx].1,
                "timestamp": inputs[idx].2,
                "branch": null::<String>,
            })
        }).collect();

        let detail = serde_json::json!({ "matches": matches });

        results.push(DetectedInsight {
            insight_type: "repeated_prompt".into(),
            severity: "warning".into(),
            title: "Repeated instruction detected".into(),
            summary: format!("Similar prompt found in {} sessions", sorted_sids.len()),
            detail_json: serde_json::to_string(&detail).unwrap_or_default(),
            fingerprint,
        });
    }

    results
}

fn detect_startup_patterns(conn: &Connection) -> Vec<DetectedInsight> {
    // Get first 5 inputs per session
    let mut stmt = match conn.prepare(
        "SELECT session_id, payload FROM session_events
         WHERE event_type = 'user_input'
         AND json_extract(payload, '$.position') <= 5
         ORDER BY session_id, created_at ASC",
    ) {
        Ok(s) => s,
        Err(_) => return vec![],
    };

    let rows: Vec<(i64, String)> = stmt
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
        .ok()
        .map(|rows| rows.filter_map(|r| r.ok()).collect())
        .unwrap_or_default();

    // Group by session
    let mut session_cmds: HashMap<i64, Vec<String>> = HashMap::new();
    for (sid, payload) in rows {
        if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&payload) {
            if let Some(text) = parsed.get("text").and_then(|t| t.as_str()) {
                session_cmds.entry(sid).or_default().push(text.to_lowercase().trim().to_string());
            }
        }
    }

    if session_cmds.len() < 3 { return vec![]; }

    // Find common command subsequences across sessions (simplified: exact prefix match)
    let sessions: Vec<(i64, Vec<String>)> = session_cmds.into_iter().collect();
    let mut common_prefix_groups: HashMap<String, Vec<i64>> = HashMap::new();

    for (sid, cmds) in &sessions {
        if cmds.is_empty() { continue; }
        let key = cmds[0..cmds.len().min(3)].join("|");
        common_prefix_groups.entry(key).or_default().push(*sid);
    }

    let mut results = vec![];
    for (prefix_key, sids) in common_prefix_groups {
        if sids.len() < 3 { continue; }

        let mut sorted_sids = sids.clone();
        sorted_sids.sort();
        let fingerprint = format!("startup_pattern:{}", sorted_sids.iter().map(|s| s.to_string()).collect::<Vec<_>>().join(","));

        let existing: bool = conn
            .query_row(
                "SELECT COUNT(*) FROM insights WHERE fingerprint = ?1 AND status = 'active'",
                rusqlite::params![fingerprint],
                |row| row.get::<_, i64>(0),
            )
            .unwrap_or(0) > 0;
        if existing { continue; }

        let commands: Vec<&str> = prefix_key.split('|').collect();
        let detail = serde_json::json!({
            "commands": commands,
            "sessions": sorted_sids.iter().map(|s| serde_json::json!({"sessionId": s, "branch": null::<String>})).collect::<Vec<_>>(),
        });

        results.push(DetectedInsight {
            insight_type: "startup_pattern".into(),
            severity: "warning".into(),
            title: "Startup routine pattern".into(),
            summary: format!("{} sessions start with similar commands", sorted_sids.len()),
            detail_json: serde_json::to_string(&detail).unwrap_or_default(),
            fingerprint,
        });
    }

    results
}

fn detect_similar_sessions(conn: &Connection) -> Vec<DetectedInsight> {
    // Get file operations per active session
    let mut stmt = match conn.prepare(
        "SELECT se.session_id, json_extract(se.payload, '$.filePath')
         FROM session_events se
         JOIN sessions s ON s.id = se.session_id
         WHERE se.event_type = 'file_operation' AND s.status = 'Running'",
    ) {
        Ok(s) => s,
        Err(_) => return vec![],
    };

    let rows: Vec<(i64, Option<String>)> = stmt
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
        .ok()
        .map(|rows| rows.filter_map(|r| r.ok()).collect())
        .unwrap_or_default();

    let mut session_files: HashMap<i64, HashSet<String>> = HashMap::new();
    for (sid, file) in rows {
        if let Some(f) = file {
            session_files.entry(sid).or_default().insert(f);
        }
    }

    let session_ids: Vec<i64> = session_files.keys().cloned().collect();
    let mut results = vec![];

    for i in 0..session_ids.len() {
        for j in (i + 1)..session_ids.len() {
            let a = &session_files[&session_ids[i]];
            let b = &session_files[&session_ids[j]];
            if a.is_empty() || b.is_empty() { continue; }

            let intersection = a.intersection(b).count();
            let union = a.union(b).count();
            let jaccard = intersection as f64 / union as f64;

            if jaccard >= 0.4 {
                let mut pair = [session_ids[i], session_ids[j]];
                pair.sort();
                let fingerprint = format!("similar_sessions:{}:{}", pair[0], pair[1]);

                let existing: bool = conn
                    .query_row(
                        "SELECT COUNT(*) FROM insights WHERE fingerprint = ?1 AND status = 'active'",
                        rusqlite::params![fingerprint],
                        |row| row.get::<_, i64>(0),
                    )
                    .unwrap_or(0) > 0;
                if existing { continue; }

                let shared: Vec<&String> = a.intersection(b).collect();
                let detail = serde_json::json!({
                    "sessionA": {"id": pair[0], "branch": null::<String>},
                    "sessionB": {"id": pair[1], "branch": null::<String>},
                    "similarity": jaccard,
                    "sharedFiles": shared,
                });

                results.push(DetectedInsight {
                    insight_type: "similar_sessions".into(),
                    severity: "suggestion".into(),
                    title: "Similar sessions detected".into(),
                    summary: format!("Sessions {} and {} share {} files", pair[0], pair[1], intersection),
                    detail_json: serde_json::to_string(&detail).unwrap_or_default(),
                    fingerprint,
                });
            }
        }
    }

    results
}

#[tauri::command]
pub async fn run_batch_analysis(
    app: tauri::AppHandle,
    db: State<'_, Mutex<Connection>>,
) -> Result<(), String> {
    // Run detection under lock (reads only, but holds lock briefly per detector)
    // Each detect_* function acquires and releases the lock independently
    // to avoid holding it during the full O(n²) analysis.
    //
    // Note: detect_* functions take &Connection directly. We acquire the lock
    // once here for simplicity, but if contention becomes an issue, refactor
    // each detector to take State<Mutex<Connection>> and lock/unlock independently.
    let all_detected = {
        let conn = db.lock().map_err(|e| format!("DB lock error: {e}"))?;
        let mut results = vec![];
        results.extend(detect_repeated_prompts(&conn));
        results.extend(detect_startup_patterns(&conn));
        results.extend(detect_similar_sessions(&conn));
        results
    }; // Lock released here before insert loop

    // Insert detected insights (re-acquire lock for writes)
    let conn = db.lock().map_err(|e| format!("DB lock error: {e}"))?;
    for detected in all_detected {
        let now = chrono::Utc::now().timestamp_millis();
        let insert_result = conn.execute(
            "INSERT OR IGNORE INTO insights (insight_type, severity, title, summary, detail_json, fingerprint, status, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'active', ?7)",
            rusqlite::params![
                detected.insight_type,
                detected.severity,
                detected.title,
                detected.summary,
                detected.detail_json,
                detected.fingerprint,
                now,
            ],
        );

        if let Ok(changes) = insert_result {
            if changes > 0 {
                let id = conn.last_insert_rowid();
                let insight = Insight {
                    id,
                    insight_type: detected.insight_type,
                    severity: detected.severity,
                    title: detected.title,
                    summary: detected.summary,
                    detail_json: detected.detail_json,
                    fingerprint: detected.fingerprint,
                    status: "active".into(),
                    created_at: now,
                    resolved_at: None,
                };

                use tauri::Emitter;
                let _ = app.emit("insight-detected", &insight);
            }
        }
    }

    Ok(())
}
```

- [ ] **Step 2: Add use statements at top of insights.rs**

Ensure these are at the top of the file (add any not already present):
```rust
use strsim::normalized_levenshtein;
use std::collections::{HashMap, HashSet};
```

- [ ] **Step 3: Register run_batch_analysis in lib.rs**

Add to invoke_handler (if not already registered in Task 2):
```rust
            commands::insights::run_batch_analysis,
```

- [ ] **Step 4: Verify compilation**

Run: `cd src-tauri && cargo check`
Expected: compiles without errors

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/commands/insights.rs src-tauri/Cargo.toml src-tauri/src/lib.rs
git commit -m "feat(insights): implement batch analysis — repeated prompts, startup patterns, similar sessions"
```

---

## Chunk 5: UI Components

### Task 9: InsightCard Component

**Files:**
- Create: `src/components/Insights/InsightCard.tsx`

- [ ] **Step 1: Create InsightCard.tsx**

```tsx
import type { Insight } from "../../types/insights";
import { InsightActions } from "./InsightActions";

const SEVERITY_STYLES: Record<string, { border: string; icon: string; iconColor: string }> = {
  alert: { border: "border-l-status-error", icon: "⚠", iconColor: "text-status-error" },
  warning: { border: "border-l-yellow-500", icon: "◇", iconColor: "text-yellow-500" },
  suggestion: { border: "border-l-status-completed", icon: "✦", iconColor: "text-status-completed" },
};

interface InsightCardProps {
  insight: Insight;
  expanded: boolean;
  onToggle: () => void;
  onDismiss: () => void;
  onApply: () => void;
}

export function InsightCard({ insight, expanded, onToggle, onDismiss, onApply }: InsightCardProps) {
  const style = SEVERITY_STYLES[insight.severity] || SEVERITY_STYLES.warning;
  let detail: Record<string, unknown> = {};
  try {
    detail = JSON.parse(insight.detail_json);
  } catch { /* ignore */ }

  return (
    <div
      className={`rounded-md border bg-surface-1 transition-all ${
        expanded ? "border-accent" : `border-surface-3 ${style.border} border-l-2`
      }`}
    >
      {/* Collapsed header — always visible */}
      <button
        onClick={onToggle}
        className="flex w-full items-center gap-2 px-3 py-2.5 text-left"
      >
        <span className={`text-sm ${style.iconColor}`}>{style.icon}</span>
        <div className="min-w-0 flex-1">
          <div className="truncate text-xs font-medium text-text-primary">{insight.title}</div>
          <div className="truncate text-[11px] text-text-secondary">{insight.summary}</div>
        </div>
        {expanded && (
          <span className="text-[10px] text-text-tertiary">▲</span>
        )}
      </button>

      {/* Expanded detail */}
      {expanded && (
        <div className="border-t border-surface-3 bg-surface-0 px-3 py-3">
          {/* Evidence section */}
          <div className="mb-3">
            <div className="mb-1.5 text-[10px] font-medium uppercase tracking-wider text-text-tertiary">
              Evidence
            </div>
            <EvidenceList insightType={insight.insight_type} detail={detail} />
          </div>

          {/* Suggestion section (for types that have LLM-generated content) */}
          {(detail as Record<string, unknown>).suggestedEntry && (
            <div className="mb-3">
              <div className="mb-1.5 text-[10px] font-medium uppercase tracking-wider text-text-tertiary">
                Suggested
              </div>
              <div className="rounded bg-surface-1 border border-surface-3 px-2.5 py-2 font-mono text-[11px] text-status-completed">
                {String((detail as Record<string, unknown>).suggestedEntry)}
              </div>
            </div>
          )}

          {/* Action buttons */}
          <InsightActions
            insightType={insight.insight_type}
            detail={detail}
            onApply={onApply}
            onDismiss={onDismiss}
          />
        </div>
      )}
    </div>
  );
}

function EvidenceList({ insightType, detail }: { insightType: string; detail: Record<string, unknown> }) {
  switch (insightType) {
    case "repeated_prompt": {
      const matches = (detail.matches as Array<{ sessionId: number; text: string; timestamp: number }>) || [];
      return (
        <div className="space-y-1">
          {matches.slice(0, 5).map((m, i) => (
            <div key={i} className="rounded bg-surface-1 border border-surface-3 px-2 py-1.5">
              <div className="flex items-center justify-between">
                <span className="text-[10px] text-accent">session-{m.sessionId}</span>
                <span className="text-[9px] text-text-tertiary">
                  {new Date(m.timestamp).toLocaleTimeString()}
                </span>
              </div>
              <div className="mt-0.5 text-[10px] italic text-text-secondary">"{m.text}"</div>
            </div>
          ))}
          {matches.length > 5 && (
            <div className="text-center text-[9px] text-text-tertiary">
              + {matches.length - 5} more
            </div>
          )}
        </div>
      );
    }

    case "file_conflict": {
      const sessions = (detail.sessions as Array<{ sessionId: number; operation: string }>) || [];
      return (
        <div className="space-y-1">
          <div className="text-[11px] font-mono text-text-primary">{String(detail.filePath)}</div>
          {sessions.map((s, i) => (
            <div key={i} className="text-[10px] text-text-secondary">
              session-{s.sessionId}: {s.operation}
            </div>
          ))}
        </div>
      );
    }

    case "cost_anomaly": {
      const d = detail as { currentCost?: number; averageCost?: number; sessionId?: number };
      return (
        <div className="text-[11px] text-text-secondary">
          <div>Session {d.sessionId}: ${(d.currentCost ?? 0).toFixed(2)} in last interval</div>
          <div>Average: ${(d.averageCost ?? 0).toFixed(2)}</div>
        </div>
      );
    }

    case "repeated_permission": {
      const d = detail as { permissionType?: string; count?: number; sessionId?: number };
      return (
        <div className="text-[11px] text-text-secondary">
          Permission "{d.permissionType}" requested {d.count} times in session {d.sessionId}
        </div>
      );
    }

    case "startup_pattern": {
      const commands = (detail.commands as string[]) || [];
      const sessions = (detail.sessions as Array<{ sessionId: number }>) || [];
      return (
        <div>
          <div className="mb-1 text-[10px] text-text-secondary">
            Found in {sessions.length} sessions:
          </div>
          <div className="space-y-0.5">
            {commands.map((cmd, i) => (
              <div key={i} className="rounded bg-surface-1 border border-surface-3 px-2 py-1 font-mono text-[10px] text-text-primary">
                {cmd}
              </div>
            ))}
          </div>
        </div>
      );
    }

    case "similar_sessions": {
      const d = detail as {
        sessionA?: { id: number };
        sessionB?: { id: number };
        similarity?: number;
        sharedFiles?: string[];
      };
      return (
        <div className="text-[11px] text-text-secondary">
          <div>Sessions {d.sessionA?.id} and {d.sessionB?.id}</div>
          <div>Similarity: {((d.similarity ?? 0) * 100).toFixed(0)}%</div>
          {d.sharedFiles && d.sharedFiles.length > 0 && (
            <div className="mt-1">
              <div className="text-[10px] text-text-tertiary">Shared files:</div>
              {d.sharedFiles.slice(0, 3).map((f, i) => (
                <div key={i} className="font-mono text-[10px]">{f}</div>
              ))}
            </div>
          )}
        </div>
      );
    }

    default:
      return <div className="text-[11px] text-text-tertiary">No detail available</div>;
  }
}
```

- [ ] **Step 2: Verify compilation**

Run: `bun run build`
Expected: no type errors

- [ ] **Step 3: Commit**

```bash
git add src/components/Insights/InsightCard.tsx
git commit -m "feat(ui): add InsightCard component with collapsed/expanded states"
```

---

### Task 10: InsightActions Component

**Files:**
- Create: `src/components/Insights/InsightActions.tsx`

- [ ] **Step 1: Create InsightActions.tsx**

```tsx
import { invoke } from "@tauri-apps/api/core";
import { useSessionStore } from "../../stores/sessionStore";
import { useFileViewerStore } from "../../stores/fileViewerStore";

interface InsightActionsProps {
  insightType: string;
  detail: Record<string, unknown>;
  onApply: () => void;
  onDismiss: () => void;
}

export function InsightActions({ insightType, detail, onApply, onDismiss }: InsightActionsProps) {
  const setActiveSession = useSessionStore((s) => s.setActiveSession);

  const handleAddToClaudeMd = async () => {
    const suggested = (detail.suggestedEntry as string) || (detail.matches as Array<{ text: string }>)?.[0]?.text;
    if (!suggested) return;

    // Find repo path from active session
    const activeData = useSessionStore.getState().getActiveSession();
    const repoPath = activeData?.repo.path;
    if (!repoPath) return;

    try {
      await invoke("append_to_file", {
        path: `${repoPath}/CLAUDE.md`,
        content: `\n${suggested}`,
      });
      onApply();
    } catch (e) {
      console.error("Failed to append to CLAUDE.md:", e);
    }
  };

  const handleCopyRule = async () => {
    const perm = detail.permissionType as string;
    if (perm) {
      await navigator.clipboard.writeText(`Allow: ${perm}`);
      onApply();
    }
  };

  const handleSwitchToSession = (sessionId: number) => {
    setActiveSession(sessionId);
  };

  switch (insightType) {
    case "repeated_prompt":
    case "startup_pattern":
      return (
        <div className="flex gap-2">
          <button
            onClick={handleAddToClaudeMd}
            className="rounded-md bg-status-completed/20 px-3 py-1.5 text-[11px] font-medium text-status-completed hover:bg-status-completed/30"
          >
            Add to CLAUDE.md
          </button>
          <button
            onClick={onDismiss}
            className="rounded-md bg-surface-2 px-3 py-1.5 text-[11px] text-text-tertiary hover:bg-surface-3"
          >
            Dismiss
          </button>
        </div>
      );

    case "repeated_permission":
      return (
        <div className="flex gap-2">
          <button
            onClick={handleCopyRule}
            className="rounded-md bg-surface-2 px-3 py-1.5 text-[11px] font-medium text-text-primary hover:bg-surface-3"
          >
            Copy allowlist rule
          </button>
          <button
            onClick={onDismiss}
            className="rounded-md bg-surface-2 px-3 py-1.5 text-[11px] text-text-tertiary hover:bg-surface-3"
          >
            Dismiss
          </button>
        </div>
      );

    case "cost_anomaly":
      return (
        <div className="flex gap-2">
          <button
            onClick={() => handleSwitchToSession(detail.sessionId as number)}
            className="rounded-md bg-accent/20 px-3 py-1.5 text-[11px] font-medium text-accent hover:bg-accent/30"
          >
            Switch to session
          </button>
          <button
            onClick={onDismiss}
            className="rounded-md bg-surface-2 px-3 py-1.5 text-[11px] text-text-tertiary hover:bg-surface-3"
          >
            Dismiss
          </button>
        </div>
      );

    case "file_conflict": {
      const sessions = (detail.sessions as Array<{ sessionId: number }>) || [];
      const filePath = detail.filePath as string;
      const handleViewDiff = () => {
        // Open the conflicting file in the FileViewer for the first involved session
        if (filePath && sessions.length > 0) {
          useFileViewerStore.getState().openFile({
            sessionId: sessions[0].sessionId,
            filePath,
          });
        }
      };
      return (
        <div className="flex flex-wrap gap-2">
          <button
            onClick={handleViewDiff}
            className="rounded-md bg-status-error/20 px-3 py-1.5 text-[11px] font-medium text-status-error hover:bg-status-error/30"
          >
            View File
          </button>
          {sessions.map((s) => (
            <button
              key={s.sessionId}
              onClick={() => handleSwitchToSession(s.sessionId)}
              className="rounded-md bg-accent/20 px-3 py-1.5 text-[11px] font-medium text-accent hover:bg-accent/30"
            >
              Session {s.sessionId}
            </button>
          ))}
          <button
            onClick={onDismiss}
            className="rounded-md bg-surface-2 px-3 py-1.5 text-[11px] text-text-tertiary hover:bg-surface-3"
          >
            Dismiss
          </button>
        </div>
      );
    }

    case "similar_sessions": {
      const sessionA = (detail.sessionA as { id: number })?.id;
      const sessionB = (detail.sessionB as { id: number })?.id;
      return (
        <div className="flex gap-2">
          {sessionA && (
            <button
              onClick={() => handleSwitchToSession(sessionA)}
              className="rounded-md bg-accent/20 px-3 py-1.5 text-[11px] font-medium text-accent hover:bg-accent/30"
            >
              Session {sessionA}
            </button>
          )}
          {sessionB && (
            <button
              onClick={() => handleSwitchToSession(sessionB)}
              className="rounded-md bg-accent/20 px-3 py-1.5 text-[11px] font-medium text-accent hover:bg-accent/30"
            >
              Session {sessionB}
            </button>
          )}
          <button
            onClick={onDismiss}
            className="rounded-md bg-surface-2 px-3 py-1.5 text-[11px] text-text-tertiary hover:bg-surface-3"
          >
            Dismiss
          </button>
        </div>
      );
    }

    default:
      return (
        <button
          onClick={onDismiss}
          className="rounded-md bg-surface-2 px-3 py-1.5 text-[11px] text-text-tertiary hover:bg-surface-3"
        >
          Dismiss
        </button>
      );
  }
}
```

- [ ] **Step 2: Verify compilation**

Run: `bun run build`
Expected: no type errors

- [ ] **Step 3: Commit**

```bash
git add src/components/Insights/InsightActions.tsx
git commit -m "feat(ui): add InsightActions component with per-type action buttons"
```

---

### Task 11: InsightsPanel Component

**Files:**
- Create: `src/components/Insights/InsightsPanel.tsx`

- [ ] **Step 1: Create InsightsPanel.tsx**

```tsx
import { useEffect, useState } from "react";
import { useInsightsStore } from "../../stores/insightsStore";
import { InsightCard } from "./InsightCard";
import { AssistantSetup } from "../Assistant/AssistantSetup";

function timeAgo(timestamp: number): string {
  const seconds = Math.floor((Date.now() - timestamp) / 1000);
  if (seconds < 60) return "Just now";
  const minutes = Math.floor(seconds / 60);
  if (minutes < 60) return `${minutes}m ago`;
  const hours = Math.floor(minutes / 60);
  if (hours < 24) return `${hours}h ago`;
  const days = Math.floor(hours / 24);
  return `${days}d ago`;
}

const SEVERITY_DOT_COLOR: Record<string, string> = {
  alert: "bg-status-error",
  warning: "bg-yellow-500",
  suggestion: "bg-status-completed",
};

export function InsightsPanel() {
  const insights = useInsightsStore((s) => s.insights);
  const expandedId = useInsightsStore((s) => s.expandedId);
  const loading = useInsightsStore((s) => s.loading);
  const initialize = useInsightsStore((s) => s.initialize);
  const toggleExpand = useInsightsStore((s) => s.toggleExpand);
  const dismissInsight = useInsightsStore((s) => s.dismissInsight);
  const applyInsight = useInsightsStore((s) => s.applyInsight);
  const [showSettings, setShowSettings] = useState(false);

  useEffect(() => {
    initialize();
  }, [initialize]);

  if (showSettings) {
    return <AssistantSetup onBack={() => setShowSettings(false)} />;
  }

  return (
    <div className="flex h-full flex-col">
      {/* Header */}
      <div className="flex items-center justify-between border-b border-surface-3 px-4 py-2.5">
        <span className="text-sm font-semibold text-text-primary">Insights</span>
        <div className="flex items-center gap-2">
          {insights.length > 0 && (
            <span className="rounded-full bg-accent/20 px-2 py-0.5 text-[10px] font-medium text-accent">
              {insights.length} active
            </span>
          )}
          <button
            onClick={() => setShowSettings(true)}
            className="text-text-tertiary hover:text-text-secondary"
            title="API settings (for LLM-generated suggestions)"
          >
            <span className="text-sm">⚙</span>
          </button>
        </div>
      </div>

      {/* Timeline feed */}
      <div className="flex-1 overflow-y-auto px-3 py-3">
        {loading ? (
          <div className="flex items-center justify-center py-12">
            <span className="text-xs text-text-tertiary">Loading...</span>
          </div>
        ) : insights.length === 0 ? (
          <div className="flex flex-col items-center justify-center py-16 text-center">
            <div className="mb-3 text-2xl opacity-30">◇</div>
            <div className="text-xs text-text-tertiary">
              No insights yet. Patterns will appear
              <br />
              as you work across sessions.
            </div>
          </div>
        ) : (
          <div className="relative">
            {/* Timeline line */}
            <div className="absolute left-[5px] top-2 bottom-2 w-px bg-surface-3" />

            {/* Cards */}
            <div className="space-y-3">
              {insights.map((insight) => (
                <div key={insight.id} className="relative pl-5">
                  {/* Timeline dot */}
                  <div
                    className={`absolute left-0 top-3 h-[10px] w-[10px] rounded-full border-2 border-surface-0 ${
                      SEVERITY_DOT_COLOR[insight.severity] || "bg-surface-3"
                    }`}
                  />
                  {/* Time label */}
                  <div className="mb-1 text-[9px] text-text-tertiary">
                    {timeAgo(insight.created_at)}
                  </div>
                  {/* Card */}
                  <InsightCard
                    insight={insight}
                    expanded={expandedId === insight.id}
                    onToggle={() => toggleExpand(insight.id)}
                    onDismiss={() => dismissInsight(insight.id)}
                    onApply={() => applyInsight(insight.id)}
                  />
                </div>
              ))}
            </div>
          </div>
        )}
      </div>
    </div>
  );
}
```

- [ ] **Step 2: Verify compilation**

Run: `bun run build`
Expected: no type errors

- [ ] **Step 3: Commit**

```bash
git add src/components/Insights/InsightsPanel.tsx
git commit -m "feat(ui): add InsightsPanel with timeline feed layout"
```

---

## Chunk 6: Integration

### Task 12: Wire Up App.tsx

**Files:**
- Modify: `src/App.tsx`

- [ ] **Step 1: Replace AssistantPanel with InsightsPanel**

In `src/App.tsx`:

1. Replace the import (line 4):
   - Old: `import { AssistantPanel } from "./components/Assistant/AssistantPanel";`
   - New: `import { InsightsPanel } from "./components/Insights/InsightsPanel";`

2. Replace the component in JSX (line 45):
   - Old: `<AssistantPanel />`
   - New: `<InsightsPanel />`

3. Update the comment (line 43):
   - Old: `{/* Right Panel — Assistant Chat (~30%) */}`
   - New: `{/* Right Panel — Insights (~30%) */}`

- [ ] **Step 2: Verify compilation**

Run: `bun run build`
Expected: no type errors

- [ ] **Step 3: Commit**

```bash
git add src/App.tsx
git commit -m "feat: replace AssistantPanel with InsightsPanel in App layout"
```

---

### Task 13: Initialize Event Capture

**Files:**
- Modify: `src/stores/sessionStore.ts`

- [ ] **Step 1: Add event capture imports and initialization**

Add import at top of `src/stores/sessionStore.ts` (after line 5):
```typescript
import { initEventCapture, recordEvent } from "../services/eventCapture";
```

In the `initialize` method (after `setActivityCallback` setup, around line 64), add:
```typescript
    // Initialize event capture for insights
    initEventCapture();
```

- [ ] **Step 2: Emit file operation events from activity callback**

Modify the `setActivityCallback` in `initialize` to also emit events. Replace lines 62-64:

```typescript
    setActivityCallback((sessionId, activity) => {
      get().updateSessionActivity(sessionId, activity);

      // Emit structured events for insights
      if (activity.action === "Editing" || activity.action === "Writing") {
        recordEvent(sessionId, "file_operation", {
          operation: activity.action === "Editing" ? "edit" : "write",
          filePath: activity.detail || "unknown",
        });
      } else if (activity.action === "Waiting for approval") {
        recordEvent(sessionId, "permission_request", {
          permissionType: "general",
        });
      }
    });
```

- [ ] **Step 3: Emit session_meta on session creation**

In `createSession` (after `startTracking` on line 139), add:
```typescript
      // Record session metadata for insights
      recordEvent(session.id, "session_meta", {
        branch: session.branch || null,
        agent: session.agent,
      });
```

- [ ] **Step 4: Trigger batch analysis on session stop**

In `stopSession` (after `invoke("stop_session", { sessionId })` on line 190), add:
```typescript
      // Trigger batch analysis when a session ends
      invoke("run_batch_analysis").catch(() => {});
```

- [ ] **Step 5: Verify compilation**

Run: `bun run build`
Expected: no type errors

- [ ] **Step 6: Verify full app builds**

Run: `bun tauri build --debug 2>&1 | tail -5` (or just `cd src-tauri && cargo build`)
Expected: compiles without errors

- [ ] **Step 7: Commit**

```bash
git add src/stores/sessionStore.ts
git commit -m "feat: wire event capture and insights into session lifecycle"
```

---

### Task 14: Manual Smoke Test

- [ ] **Step 1: Start dev server**

Run: `bun tauri dev`

- [ ] **Step 2: Verify the right panel shows "Insights" header with empty state**

Expected: "No insights yet. Patterns will appear as you work across sessions." with ◇ icon

- [ ] **Step 3: Create 2+ sessions and modify the same file in both**

Expected: A "File conflict" insight card appears in the timeline with red severity dot

- [ ] **Step 4: Click a card to expand, verify detail + action buttons appear**

Expected: Evidence section shows file path and session IDs, action buttons (session links + dismiss) are visible

- [ ] **Step 5: Click Dismiss, verify card disappears**

Expected: Card removed from feed
