# Insights Panel — Design Spec

> Replace the right-side Assistant chat panel with an actionable Insights feed that detects patterns across sessions and surfaces suggestions to accelerate user workflows.

## Context

The current right panel (`AssistantPanel`) is a generic LLM chat interface. Users rarely interact with it meaningfully — they talk to agents directly in terminal sessions. Meanwhile, when running multiple sessions, users repeatedly type similar instructions, encounter cross-session file conflicts, and miss cost anomalies — all situations where automated detection and one-click actions would save significant time.

## Decision Record

| Decision | Choice | Alternatives Considered |
|----------|--------|------------------------|
| Panel role | Fully replace assistant chat | Coexist with chat; insights as tab |
| Layout | Chronological timeline feed | Grouped by type (dashboard-style) |
| Card interaction | Inline expand in place | Slide-over detail panel |
| Detection approach | Event-driven + batch analysis | Real-time streaming; pure rule-based |
| Architecture | Hybrid (frontend real-time rules + Rust batch analysis) | Pure frontend; pure Rust |
| LLM role | Generate suggestion text only, never used for detection | LLM-driven detection; no LLM |

## Insight Types

Six insight types, ordered by detection complexity:

| # | Type | Trigger | Severity |
|---|------|---------|----------|
| 1 | **Repeated Prompt** | Same/similar instruction appears in ≥3 sessions | warning (amber) |
| 2 | **Startup Pattern** | ≥3 sessions begin with the same command sequence | warning (amber) |
| 3 | **Repeated Permission** | Same permission type requested ≥3 times in one session | warning (amber) |
| 4 | **Cost Anomaly** | 10-min cost > 3× session's historical average | alert (red) |
| 5 | **File Conflict** | Same file written/edited in ≥2 active sessions | alert (red) |
| 6 | **Similar Sessions** | Two sessions share branch-name pattern, overlapping file set, or similar initial prompt | suggestion (green) |

## Architecture

### Overview

```
┌─────────────────────────────────────────────────┐
│                   Frontend                       │
│                                                  │
│  PTY Output ──► ptyOutputParser ──► EventCapture │
│                                        │         │
│                    ┌───────────────────┤         │
│                    ▼                   ▼         │
│            insightsStore         invoke()        │
│         (real-time rules)       flush events     │
│            │                        │            │
│            ▼                        │            │
│       InsightsPanel                 │            │
│            ▲                        │            │
│            │ Tauri event            │            │
│            │ "insight-detected"     │            │
└────────────┼────────────────────────┼────────────┘
             │                        │
┌────────────┼────────────────────────┼────────────┐
│            │        Rust Backend    ▼            │
│            │                  session_events     │
│            │                  (SQLite)           │
│         analysis                    │            │
│         engine  ◄───────────────────┘            │
│            │                                     │
│            ▼                                     │
│        insights (SQLite)                         │
└──────────────────────────────────────────────────┘
```

### Event Capture Layer

**Source: Enhanced `ptyOutputParser`**

The existing parser already detects Read/Edit/Write/Bash/Permission patterns. We extend it to also capture:
- **User prompts**: Extract text after Claude Code's input prompt markers (`❯`, `>`, or the human turn delimiter). These are already echoed in PTY output as complete lines, avoiding the raw keystroke buffering problem.
- **Session start sequences**: The first N user inputs after a session begins, tagged with ordinal position.

**New service: `eventCapture.ts`**

Subscribes to ptyOutputParser callbacks and sessionStore changes. Normalizes raw activity into structured events and:
1. Writes to `insightsStore` (Zustand) for real-time rule evaluation
2. Batches events and flushes to Rust/SQLite every 30 seconds via `invoke('record_session_events', { events })`

**Event schema (TypeScript):**

```typescript
interface SessionEvent {
  id?: number;              // assigned by SQLite
  sessionId: string;
  eventType: 'user_input' | 'permission_request' | 'file_operation' | 'cost_update' | 'session_meta';
  payload: Record<string, unknown>;
  // payload examples:
  //   user_input:         { text: "use TDD approach", position: 3 }
  //   permission_request: { permissionType: "bash", count: 1 }
  //   file_operation:     { operation: "edit", filePath: "src/utils/api.ts" }
  //   cost_update:        { cost: 0.42, windowMinutes: 10 }
  //   session_meta:       { branch: "feat/auth", agent: "claude", description: "..." }
  createdAt: number;        // Unix timestamp ms
}
```

### Storage Layer

**New SQLite table: `session_events`**

```sql
CREATE TABLE session_events (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
  event_type TEXT NOT NULL,
  payload TEXT NOT NULL,  -- JSON
  created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);
CREATE INDEX idx_events_session ON session_events(session_id);
CREATE INDEX idx_events_type ON session_events(event_type);
```

**New SQLite table: `insights`**

```sql
CREATE TABLE insights (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  insight_type TEXT NOT NULL,  -- repeated_prompt | startup_pattern | repeated_permission | cost_anomaly | file_conflict | similar_sessions
  severity TEXT NOT NULL,      -- warning | alert | suggestion
  title TEXT NOT NULL,
  summary TEXT NOT NULL,
  detail_json TEXT NOT NULL,   -- full evidence + suggested action, JSON
  status TEXT NOT NULL DEFAULT 'active',  -- active | applied | dismissed | expired
  created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
  resolved_at TIMESTAMP
);
```

### Analysis Engine

**Frontend real-time rules (in `insightsStore`):**

These run on every relevant event, no batch delay:

1. **File Conflict (#5)**: Maintain `Map<filePath, Set<sessionId>>`. On each `file_operation` event with operation=edit|write, update the map. If a file has >1 session, emit insight. Clear entries when sessions complete.

2. **Cost Anomaly (#4)**: On each `cost_update`, compare current 10-min window cost against the session's running average. If current > 3× average and absolute value > $0.50 (to avoid noise on tiny amounts), emit insight.

3. **Repeated Permission (#3)**: Per-session counter `Map<sessionId, Map<permissionType, number>>`. On each `permission_request`, increment. If count ≥ 3 for same type in same session, emit insight.

**Rust batch analysis (triggered every 5 minutes + on session end):**

4. **Repeated Prompt (#1)**: Query all `user_input` events from the last 7 days. Normalize (lowercase, trim, remove punctuation). Group by normalized text using Levenshtein distance with threshold 0.7. If a cluster has entries from ≥3 distinct sessions, emit insight. Call LLM to generate a suggested CLAUDE.md entry from the cluster.

5. **Startup Pattern (#2)**: For each session, take the first 5 `user_input` events (position ≤ 5). Compare sequences across sessions using Longest Common Subsequence. If ≥3 sessions share ≥3 commands in common, emit insight.

6. **Similar Sessions (#8)**: For active sessions, compare pairwise:
   - Branch name similarity (edit distance on branch name)
   - File set overlap (Jaccard index on files touched)
   - Initial prompt similarity (edit distance on first user_input)
   - Weighted score > 0.6 → emit insight

**Rust → Frontend communication**: `app_handle.emit("insight-detected", insight_payload)`. Frontend listens via `listen("insight-detected", callback)`.

### New Rust Commands

```rust
#[tauri::command]
fn record_session_events(db: State<Database>, events: Vec<SessionEvent>) -> Result<(), String>

#[tauri::command]
fn get_insights(db: State<Database>, status: Option<String>) -> Result<Vec<Insight>, String>

#[tauri::command]
fn update_insight_status(db: State<Database>, id: i64, status: String) -> Result<(), String>

#[tauri::command]
fn run_batch_analysis(app: AppHandle, db: State<Database>) -> Result<(), String>

#[tauri::command]
fn generate_insight_suggestion(config: State<AssistantConfig>, insight_id: i64, prompt_cluster: Vec<String>) -> Result<String, String>
```

### LLM Usage

LLM is called **only** when generating human-readable suggestion text, never for detection:

| Trigger | LLM Prompt | Expected Output |
|---------|-----------|-----------------|
| Repeated prompt cluster detected | "Summarize these repeated instructions into a single CLAUDE.md entry: [list]" | 1-2 sentence directive |
| Startup pattern detected | "Summarize this startup command sequence into a reusable instruction: [list]" | Shell command or CLAUDE.md entry |

Uses the user's existing assistant API config (provider + API key + model). Falls back to showing raw evidence if no API key configured.

## UI Components

### Files to Create

| File | Purpose |
|------|---------|
| `src/components/Insights/InsightsPanel.tsx` | Main panel (replaces AssistantPanel position in App.tsx) |
| `src/components/Insights/InsightCard.tsx` | Single insight card (collapsed + expanded states) |
| `src/components/Insights/InsightActions.tsx` | Action buttons per insight type |
| `src/stores/insightsStore.ts` | Zustand store: insights list, real-time rules, event buffer |
| `src/services/eventCapture.ts` | Event normalization + batching + flush to Rust |

### Files to Modify

| File | Change |
|------|--------|
| `src/App.tsx` | Replace `<AssistantPanel />` with `<InsightsPanel />` |
| `src/services/ptyOutputParser.ts` | Add user prompt extraction pattern |
| `src-tauri/src/commands/db.rs` | Add migration v4 (session_events + insights tables) |
| `src-tauri/src/lib.rs` | Register new commands |

### Files to Delete (or keep for reference)

| File | Reason |
|------|--------|
| `src/components/Assistant/AssistantChat.tsx` | Replaced by InsightsPanel |
| `src/components/Assistant/AssistantMessage.tsx` | No longer needed |
| `src/stores/assistantStore.ts` | Replaced by insightsStore (keep config portion for LLM access) |

Note: `AssistantSetup.tsx` should be preserved — the user still needs to configure an API key for LLM-generated suggestions. It can be shown via a settings icon in the InsightsPanel header.

### InsightsPanel Layout

```
┌──────────────────────────┐
│ Insights          ⚙ ··· │  ← header (settings gear opens AssistantSetup)
│ ┌─ 3 new ─────────────┐ │
├──────────────────────────┤
│ ○ Just now              │  ← timeline dot (color = severity)
│ ┌────────────────────┐  │
│ │ ⚠ File conflict     │  │  ← collapsed card
│ │ src/utils/api.ts    │  │
│ │ 2 sessions          │  │
│ └────────────────────┘  │
│                          │
│ ○ 2 min ago             │
│ ┌────────────────────┐  │
│ │ ◇ Repeated prompt  ▲│  │  ← expanded card
│ ├────────────────────┤  │
│ │ Evidence:           │  │
│ │  session-1: "..."   │  │
│ │  session-3: "..."   │  │
│ │  session-5: "..."   │  │
│ │                     │  │
│ │ Suggested:          │  │
│ │ ┌─────────────────┐ │  │
│ │ │ Always use TDD  │ │  │  ← editable suggestion
│ │ └─────────────────┘ │  │
│ │                     │  │
│ │ [Add to CLAUDE.md]  │  │  ← primary action
│ │ [Edit] [Dismiss]    │  │  ← secondary actions
│ └────────────────────┘  │
│                          │
│ ○ 10 min ago            │
│ ┌────────────────────┐  │
│ │ ✦ Similar sessions  │  │
│ │ session-3 & -7      │  │
│ └────────────────────┘  │
└──────────────────────────┘
```

### InsightCard States

**Collapsed** (default):
- Left border color indicates severity (amber/red/green)
- Icon + title + one-line summary
- Click anywhere to expand

**Expanded**:
- Blue border replaces severity color (indicates focus)
- Evidence section: list of matched events with session links
- Suggestion section: LLM-generated text in an editable code block (for types 1, 2)
- Action buttons row at bottom
- Click header area to collapse

### Insight Actions by Type

| Type | Primary Action | Secondary Actions |
|------|---------------|-------------------|
| Repeated Prompt | `Add to CLAUDE.md` | `Edit`, `Dismiss` |
| Startup Pattern | `Add to CLAUDE.md` | `Dismiss` |
| Repeated Permission | `Copy allowlist rule` | `Dismiss` |
| Cost Anomaly | `Switch to session` | `Dismiss` |
| File Conflict | `View Diff` | `Switch to session`, `Dismiss` |
| Similar Sessions | `Switch to session` | `Dismiss` |

**Action implementations:**
- `Add to CLAUDE.md`: `invoke('append_to_file', { path: "<repo>/CLAUDE.md", content })` — appends the suggestion text
- `Copy allowlist rule`: `navigator.clipboard.writeText(rule)`
- `Switch to session`: `sessionStore.setActiveSession(sessionId)`
- `View Diff`: `fileViewerStore.openDiff(filePath, sessionA, sessionB)`
- `Dismiss`: `invoke('update_insight_status', { id, status: 'dismissed' })`

## Empty & Edge States

- **No insights yet**: Show a calm placeholder — "No insights yet. Patterns will appear as you work across sessions." with a subtle icon.
- **All dismissed**: Same placeholder.
- **No API key configured**: Insights still detect and display, but suggestion text shows raw evidence instead of LLM-generated summary. Settings gear pulses subtly to hint at configuration.
- **Single session only**: File conflict and similar session detection are disabled. Other insights still work.
- **Session reconnect on app restart**: Existing insights loaded from SQLite. Event history preserved. Real-time rules re-initialize from persisted events.

## Performance Considerations

- **Event flush interval**: 30 seconds. Buffer max 200 events before force-flush.
- **Batch analysis interval**: 5 minutes (configurable). Also triggers on session end.
- **Levenshtein computation**: O(n²) pairwise on user_input events. Cap at last 7 days of data and max 500 events per analysis run. For 500 events, ~125K comparisons — acceptable for batch.
- **SQLite writes**: Batched inserts (30s interval), not per-event. Minimal write amplification.
- **Frontend memory**: `insightsStore` only holds active insights + current event buffer. Historical data stays in SQLite.

## Out of Scope (Future)

- Skill creation flow (button placeholder only)
- Cross-repo insight correlation
- Insight notification sounds/system notifications
- Custom rule configuration by users
- Insight analytics/history view
