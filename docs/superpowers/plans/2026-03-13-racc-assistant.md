# Racc Assistant Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a global AI assistant ("butler") in Racc's right panel that summarizes coding agent diffs and triages review risk, powered by pi-ai/pi-agent-core running as a Tauri sidecar.

**Architecture:** A Bun-compiled TypeScript sidecar communicates with the Rust backend via JSON lines over stdin/stdout. The Rust backend manages SQLite persistence, session-to-path resolution, and sidecar lifecycle. The React frontend renders a chat UI in the right panel with streaming markdown responses.

**Tech Stack:** Rust (Tauri 2.x commands, rusqlite), TypeScript/Bun (pi-ai, pi-agent-core), React 19 (Zustand, react-markdown, Tailwind)

**Spec:** `docs/superpowers/specs/2026-03-13-racc-assistant-design.md`

---

## File Structure

### New Files

| File | Responsibility |
|------|----------------|
| `sidecar/package.json` | Sidecar dependencies (pi-ai, pi-agent-core) |
| `sidecar/tsconfig.json` | TypeScript config for sidecar |
| `sidecar/src/index.ts` | Entry point — stdin/stdout JSON line protocol loop |
| `sidecar/src/protocol.ts` | Message type definitions and JSON parsing |
| `sidecar/src/agent.ts` | pi-agent-core agent setup, system prompt, tool wiring |
| `sidecar/src/tools.ts` | Tool definitions for get_all_sessions, get_session_diff, get_session_costs |
| `sidecar/build.sh` | Cross-platform bun compile script |
| `src-tauri/src/commands/assistant.rs` | Rust commands: sidecar spawn, message relay, SQLite CRUD |
| `src/stores/assistantStore.ts` | Zustand store for assistant state |
| `src/types/assistant.ts` | TypeScript types for assistant messages and config |
| `src/components/Assistant/AssistantPanel.tsx` | Main panel — switches between Setup and Chat views |
| `src/components/Assistant/AssistantSetup.tsx` | Provider dropdown, API key input, model picker |
| `src/components/Assistant/AssistantChat.tsx` | Message list + input field + quick actions |
| `src/components/Assistant/AssistantMessage.tsx` | Single message bubble with react-markdown |

### Modified Files

| File | Change |
|------|--------|
| `src-tauri/src/commands/db.rs` | Add v3 migration: `assistant_messages` + `assistant_config` tables |
| `src-tauri/src/commands/mod.rs` | Add `pub mod assistant;` |
| `src-tauri/src/lib.rs` | Register assistant commands, manage sidecar state |
| `src-tauri/tauri.conf.json` | Add `externalBin` for sidecar |
| `src-tauri/capabilities/default.json` | Fix "OTTE" → "Racc" description |
| `src/App.tsx` | Replace `<ActivityLog />` with `<AssistantPanel />` |
| `package.json` | Add `react-markdown` and `@tailwindcss/typography` dependencies |
| `tailwind.config.ts` | Register `@tailwindcss/typography` plugin |

### Removed Files

| File | Reason |
|------|--------|
| `src/components/ActivityLog/ActivityLog.tsx` | Replaced by AssistantPanel |

---

## Chunk 1: Database & Rust Backend

### Task 1: SQLite Schema Migration (v3)

**Files:**
- Modify: `src-tauri/src/commands/db.rs`

- [ ] **Step 1: Add v3 migration block after the existing v2 block**

In `src-tauri/src/commands/db.rs`, add after the `if version < 2` block (after line 82):

```rust
if version < 3 {
    conn.execute_batch(
        "
        BEGIN;

        CREATE TABLE IF NOT EXISTS assistant_messages (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            role TEXT NOT NULL,
            content TEXT NOT NULL,
            tool_name TEXT,
            tool_call_id TEXT,
            created_at TEXT NOT NULL DEFAULT (datetime('now'))
        );

        CREATE TABLE IF NOT EXISTS assistant_config (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL
        );

        PRAGMA user_version = 3;

        COMMIT;
        ",
    )
    .map_err(|e| format!("Migration v3 failed: {e}"))?;
}
```

- [ ] **Step 2: Verify Rust compiles**

Run: `cd src-tauri && cargo check`
Expected: compiles with no errors

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/commands/db.rs
git commit -m "feat(db): add v3 migration for assistant_messages and assistant_config tables"
```

---

### Task 2: Assistant Rust Module — Data Handlers

**Files:**
- Create: `src-tauri/src/commands/assistant.rs`
- Modify: `src-tauri/src/commands/mod.rs`
- Modify: `src-tauri/Cargo.toml`

- [ ] **Step 1: Add chrono dependency to Cargo.toml**

In `src-tauri/Cargo.toml`, add under `[dependencies]`:

```toml
chrono = { version = "0.4", features = ["serde"] }
```

- [ ] **Step 2: Add assistant module to mod.rs**

In `src-tauri/src/commands/mod.rs`, add:

```rust
pub mod assistant;
```

- [ ] **Step 3: Create assistant.rs with config and message CRUD commands**

Create `src-tauri/src/commands/assistant.rs`:

```rust
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::sync::Mutex;

// --- Types ---

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssistantMessage {
    pub id: i64,
    pub role: String,
    pub content: String,
    pub tool_name: Option<String>,
    pub tool_call_id: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssistantConfig {
    pub provider: Option<String>,
    pub api_key: Option<String>,
    pub model: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SessionInfo {
    pub id: i64,
    pub status: String,
    pub agent: String,
    pub branch: Option<String>,
    pub repo_name: String,
    pub repo_path: String,
    pub worktree_path: Option<String>,
    pub elapsed_minutes: i64,
    pub created_at: String,
}

// --- Helper: resolve session ID to filesystem path ---

fn resolve_session_path(conn: &Connection, session_id: i64) -> Result<String, String> {
    let (worktree_path, repo_id): (Option<String>, i64) = conn
        .query_row(
            "SELECT worktree_path, repo_id FROM sessions WHERE id = ?1",
            [session_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .map_err(|e| format!("Session not found: {e}"))?;

    if let Some(wt) = worktree_path {
        return Ok(wt);
    }

    let repo_path: String = conn
        .query_row("SELECT path FROM repos WHERE id = ?1", [repo_id], |row| {
            row.get(0)
        })
        .map_err(|e| format!("Repo not found: {e}"))?;

    Ok(repo_path)
}

// --- Tauri Commands ---

#[tauri::command]
pub async fn get_assistant_config(
    db: tauri::State<'_, Mutex<Connection>>,
) -> Result<AssistantConfig, String> {
    let conn = db.lock().map_err(|e| e.to_string())?;

    let get_val = |key: &str| -> Option<String> {
        conn.query_row(
            "SELECT value FROM assistant_config WHERE key = ?1",
            [key],
            |row| row.get(0),
        )
        .ok()
    };

    Ok(AssistantConfig {
        provider: get_val("provider"),
        api_key: get_val("api_key"),
        model: get_val("model"),
    })
}

#[tauri::command]
pub async fn set_assistant_config(
    db: tauri::State<'_, Mutex<Connection>>,
    provider: String,
    api_key: String,
    model: String,
) -> Result<(), String> {
    let conn = db.lock().map_err(|e| e.to_string())?;

    let upsert = |key: &str, value: &str| -> Result<(), String> {
        conn.execute(
            "INSERT INTO assistant_config (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            rusqlite::params![key, value],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    };

    upsert("provider", &provider)?;
    upsert("api_key", &api_key)?;
    upsert("model", &model)?;

    Ok(())
}

#[tauri::command]
pub async fn save_assistant_message(
    db: tauri::State<'_, Mutex<Connection>>,
    role: String,
    content: String,
    tool_name: Option<String>,
    tool_call_id: Option<String>,
) -> Result<AssistantMessage, String> {
    let conn = db.lock().map_err(|e| e.to_string())?;

    conn.execute(
        "INSERT INTO assistant_messages (role, content, tool_name, tool_call_id) VALUES (?1, ?2, ?3, ?4)",
        rusqlite::params![role, content, tool_name, tool_call_id],
    )
    .map_err(|e| e.to_string())?;

    let id = conn.last_insert_rowid();
    let created_at: String = conn
        .query_row(
            "SELECT created_at FROM assistant_messages WHERE id = ?1",
            [id],
            |row| row.get(0),
        )
        .map_err(|e| e.to_string())?;

    Ok(AssistantMessage {
        id,
        role,
        content,
        tool_name,
        tool_call_id,
        created_at,
    })
}

#[tauri::command]
pub async fn get_assistant_messages(
    db: tauri::State<'_, Mutex<Connection>>,
    limit: i64,
) -> Result<Vec<AssistantMessage>, String> {
    let conn = db.lock().map_err(|e| e.to_string())?;

    let mut stmt = conn
        .prepare(
            "SELECT id, role, content, tool_name, tool_call_id, created_at
             FROM assistant_messages ORDER BY id DESC LIMIT ?1",
        )
        .map_err(|e| e.to_string())?;

    let messages: Vec<AssistantMessage> = stmt
        .query_map([limit], |row| {
            Ok(AssistantMessage {
                id: row.get(0)?,
                role: row.get(1)?,
                content: row.get(2)?,
                tool_name: row.get(3)?,
                tool_call_id: row.get(4)?,
                created_at: row.get(5)?,
            })
        })
        .map_err(|e| e.to_string())?
        .filter_map(|r| r.ok())
        .collect();

    // Reverse to get chronological order (we queried DESC for LIMIT)
    let mut messages = messages;
    messages.reverse();
    Ok(messages)
}

#[tauri::command]
pub async fn get_all_sessions_for_assistant(
    db: tauri::State<'_, Mutex<Connection>>,
) -> Result<Vec<SessionInfo>, String> {
    let conn = db.lock().map_err(|e| e.to_string())?;

    let mut stmt = conn
        .prepare(
            "SELECT s.id, s.status, s.agent, s.branch, r.name, r.path, s.worktree_path, s.created_at
             FROM sessions s JOIN repos r ON s.repo_id = r.id
             ORDER BY s.created_at DESC",
        )
        .map_err(|e| e.to_string())?;

    let now = chrono::Utc::now();
    let sessions: Vec<SessionInfo> = stmt
        .query_map([], |row| {
            let created_at: String = row.get(7)?;
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, Option<String>>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, Option<String>>(6)?,
                created_at,
            ))
        })
        .map_err(|e| e.to_string())?
        .filter_map(|r| r.ok())
        .map(|(id, status, agent, branch, repo_name, repo_path, worktree_path, created_at)| {
            // SQLite datetime('now') produces "YYYY-MM-DD HH:MM:SS" format
            let elapsed = chrono::NaiveDateTime::parse_from_str(&created_at, "%Y-%m-%d %H:%M:%S")
                .map(|ndt| (now - ndt.and_utc()).num_minutes())
                .unwrap_or(0);

            SessionInfo {
                id,
                status,
                agent,
                branch,
                repo_name,
                repo_path,
                worktree_path,
                elapsed_minutes: elapsed,
                created_at,
            }
        })
        .collect();

    Ok(sessions)
}

#[tauri::command]
pub async fn get_session_diff_for_assistant(
    db: tauri::State<'_, Mutex<Connection>>,
    session_id: i64,
) -> Result<String, String> {
    let path = {
        let conn = db.lock().map_err(|e| e.to_string())?;
        resolve_session_path(&conn, session_id)?
    };

    let output = std::process::Command::new("git")
        .args(["diff", "HEAD"])
        .current_dir(&path)
        .output()
        .map_err(|e| format!("Failed to get diff: {e}"))?;

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

#[tauri::command]
pub async fn get_session_costs_for_assistant(
    db: tauri::State<'_, Mutex<Connection>>,
    session_id: i64,
) -> Result<String, String> {
    let path = {
        let conn = db.lock().map_err(|e| e.to_string())?;
        resolve_session_path(&conn, session_id)?
    };

    // Reuse existing cost logic by invoking get_project_costs
    let costs = crate::commands::cost::get_project_costs(path).await?;
    serde_json::to_string(&costs).map_err(|e| e.to_string())
}
```

- [ ] **Step 4: Register assistant commands in lib.rs**

In `src-tauri/src/lib.rs`, add the assistant commands to the `invoke_handler`:

```rust
commands::assistant::get_assistant_config,
commands::assistant::set_assistant_config,
commands::assistant::save_assistant_message,
commands::assistant::get_assistant_messages,
commands::assistant::get_all_sessions_for_assistant,
commands::assistant::get_session_diff_for_assistant,
commands::assistant::get_session_costs_for_assistant,
```

- [ ] **Step 5: Verify Rust compiles**

Run: `cd src-tauri && cargo check`
Expected: compiles with no errors

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/commands/assistant.rs src-tauri/src/commands/mod.rs src-tauri/src/lib.rs src-tauri/Cargo.toml
git commit -m "feat(assistant): add Rust commands for config, messages, session data"
```

---

### Task 3: Tauri Configuration Updates

**Files:**
- Modify: `src-tauri/tauri.conf.json`
- Modify: `src-tauri/capabilities/default.json`

- [ ] **Step 1: Add externalBin to tauri.conf.json**

In `src-tauri/tauri.conf.json`, add `"externalBin"` inside the `"bundle"` object:

Add `"externalBin"` as a new key in the existing `"bundle"` object, keeping all other keys unchanged:

```json
"externalBin": ["binaries/racc-assistant"]
```

- [ ] **Step 2: Fix OTTE → Racc in capabilities**

In `src-tauri/capabilities/default.json`, change line 4:

```json
"description": "Default permissions for Racc",
```

- [ ] **Step 3: Create empty binaries directory with .gitkeep**

```bash
mkdir -p src-tauri/binaries
touch src-tauri/binaries/.gitkeep
```

- [ ] **Step 4: Commit**

```bash
git add src-tauri/tauri.conf.json src-tauri/capabilities/default.json src-tauri/binaries/.gitkeep
git commit -m "chore: configure Tauri sidecar binaries and fix OTTE description"
```

---

## Chunk 2: Sidecar TypeScript Project

### Task 4: Sidecar Project Scaffolding

**Files:**
- Create: `sidecar/package.json`
- Create: `sidecar/tsconfig.json`
- Create: `sidecar/src/protocol.ts`

- [ ] **Step 1: Create sidecar/package.json**

```json
{
  "name": "racc-assistant",
  "version": "0.1.0",
  "private": true,
  "type": "module",
  "scripts": {
    "build": "bash build.sh",
    "dev": "bun run src/index.ts"
  },
  "dependencies": {
    "@mariozechner/pi-ai": "^0.57.1",
    "@mariozechner/pi-agent-core": "^0.57.1",
    "@sinclair/typebox": "^0.34.0"
  },
  "devDependencies": {
    "@types/node": "^22.0.0",
    "typescript": "^5.8.0"
  }
}
```

- [ ] **Step 2: Create sidecar/tsconfig.json**

```json
{
  "compilerOptions": {
    "target": "ESNext",
    "module": "ESNext",
    "moduleResolution": "bundler",
    "strict": true,
    "esModuleInterop": true,
    "outDir": "dist",
    "rootDir": "src",
    "skipLibCheck": true
  },
  "include": ["src"]
}
```

- [ ] **Step 3: Create sidecar/src/protocol.ts**

```typescript
// --- Inbound messages (Rust → Sidecar) ---

export type InboundMessage =
  | { type: "user_message"; content: string }
  | { type: "tool_result"; call_id: string; content: string }
  | { type: "set_config"; provider: string; api_key: string; model: string }
  | { type: "history"; messages: HistoryMessage[] }
  | { type: "shutdown" };

export interface HistoryMessage {
  role: "user" | "assistant" | "tool_call" | "tool_result";
  content: string;
  tool_name?: string;
  tool_call_id?: string;
}

// --- Outbound messages (Sidecar → Rust) ---

export type OutboundMessage =
  | { type: "chunk"; text: string }
  | { type: "tool_call"; id: string; name: string; args: Record<string, unknown> }
  | { type: "done"; usage: { input_tokens: number; output_tokens: number; cost_usd: number } }
  | { type: "error"; message: string }
  | { type: "models"; models: { id: string; name: string }[] };

export function sendMessage(msg: OutboundMessage): void {
  process.stdout.write(JSON.stringify(msg) + "\n");
}

export function parseInbound(line: string): InboundMessage | null {
  try {
    return JSON.parse(line) as InboundMessage;
  } catch {
    return null;
  }
}
```

- [ ] **Step 4: Install sidecar dependencies**

```bash
cd sidecar && bun install
```

- [ ] **Step 5: Verify TypeScript compiles**

```bash
cd sidecar && npx tsc --noEmit
```

Expected: no errors

- [ ] **Step 6: Commit**

```bash
git add sidecar/package.json sidecar/tsconfig.json sidecar/src/protocol.ts sidecar/bun.lockb
git commit -m "feat(sidecar): scaffold TypeScript project with protocol types"
```

---

### Task 5: Sidecar Agent Setup and Tools

**Files:**
- Create: `sidecar/src/tools.ts`
- Create: `sidecar/src/agent.ts`

- [ ] **Step 1: Create sidecar/src/tools.ts**

Tool definitions that relay execution back to Rust via the protocol:

```typescript
import { Type } from "@sinclair/typebox";
import type { AgentTool, AgentToolResult } from "@mariozechner/pi-agent-core";
import { sendMessage } from "./protocol.js";

// Pending tool calls waiting for results from Rust
const pendingToolCalls = new Map<string, {
  resolve: (result: string) => void;
}>();

export function resolveToolCall(callId: string, content: string): void {
  const pending = pendingToolCalls.get(callId);
  if (pending) {
    pending.resolve(content);
    pendingToolCalls.delete(callId);
  }
}

function createRelayTool(
  name: string,
  description: string,
  label: string,
  parameters: any,
): AgentTool<any> {
  return {
    name,
    description,
    label,
    parameters,
    execute: async (toolCallId, params): Promise<AgentToolResult<any>> => {
      return new Promise((resolve) => {
        pendingToolCalls.set(toolCallId, {
          resolve: (content: string) => {
            resolve({
              content: [{ type: "text", text: content }],
              details: {},
            });
          },
        });

        sendMessage({
          type: "tool_call",
          id: toolCallId,
          name,
          args: params,
        });
      });
    },
  };
}

export const tools: AgentTool<any>[] = [
  createRelayTool(
    "get_all_sessions",
    "Get a list of all coding agent sessions with their status, branch, repo, and elapsed time. Use this to understand what agents are currently running or have completed.",
    "List all sessions",
    Type.Object({}),
  ),
  createRelayTool(
    "get_session_diff",
    "Get the git diff (changes) for a specific session by its ID. Returns the raw git diff HEAD output showing all file changes.",
    "Get session diff",
    Type.Object({
      session_id: Type.Number({ description: "The session ID to get the diff for" }),
    }),
  ),
  createRelayTool(
    "get_session_costs",
    "Get the token usage and estimated cost for a specific session by its ID. Note: costs are per-project, not per-session, so multiple sessions in the same project may show aggregated costs.",
    "Get session costs",
    Type.Object({
      session_id: Type.Number({ description: "The session ID to get costs for" }),
    }),
  ),
];
```

- [ ] **Step 2: Create sidecar/src/agent.ts**

```typescript
import type { AgentState, AgentLoopConfig, AgentMessage } from "@mariozechner/pi-agent-core";
import type { Message } from "@mariozechner/pi-ai";
import { tools } from "./tools.js";
import type { HistoryMessage } from "./protocol.js";

const SYSTEM_PROMPT = `You are the Racc assistant — a global operations butler for a developer running multiple AI coding agents in parallel.

Today's date: ${new Date().toISOString().split("T")[0]}

Your primary job: help the developer understand and review what their agents have done, without requiring them to read every line of every diff.

When summarizing changes:
- Lead with a high-level summary (what changed, why it likely changed)
- Categorize files by review priority:
  HIGH: security-sensitive, architectural, config, database
  MEDIUM: business logic, API changes
  LOW: tests, types, formatting, generated files
- Flag specific concerns (unparameterized SQL, hardcoded secrets, missing error handling, breaking API changes)
- Be concise — the developer has multiple agents to review

You have access to all sessions, their diffs, and their costs. Answer questions about any session's work.`;

export function createAgentState(): AgentState {
  return {
    systemPrompt: SYSTEM_PROMPT,
    model: null as any, // Set when config is received
    thinkingLevel: "off",
    tools,
    messages: [],
    isStreaming: false,
    streamMessage: null,
    pendingToolCalls: new Set(),
  };
}

export function hydrateHistory(state: AgentState, messages: HistoryMessage[]): void {
  for (const msg of messages) {
    if (msg.role === "user") {
      state.messages.push({
        role: "user",
        content: msg.content,
        timestamp: Date.now(),
      });
    } else if (msg.role === "assistant") {
      state.messages.push({
        role: "assistant",
        content: [{ type: "text", text: msg.content }],
        api: "openai-completions",
        provider: "openrouter",
        model: "",
        usage: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0, totalTokens: 0, cost: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0, total: 0 } },
        stopReason: "stop",
        timestamp: Date.now(),
      });
    }
    // tool_call and tool_result are omitted in v1 hydration for simplicity
  }
}

export function createLoopConfig(state: AgentState, apiKey: string): AgentLoopConfig {
  return {
    model: state.model,
    apiKey,
    convertToLlm: (messages: AgentMessage[]): Message[] => {
      return messages.filter(
        (m): m is Message => "role" in m && (m.role === "user" || m.role === "assistant" || m.role === "toolResult"),
      );
    },
  };
}
```

- [ ] **Step 3: Verify TypeScript compiles**

```bash
cd sidecar && npx tsc --noEmit
```

- [ ] **Step 4: Commit**

```bash
git add sidecar/src/tools.ts sidecar/src/agent.ts
git commit -m "feat(sidecar): add agent setup, system prompt, and relay tools"
```

---

### Task 6: Sidecar Entry Point and Main Loop

**Files:**
- Create: `sidecar/src/index.ts`

- [ ] **Step 1: Create sidecar/src/index.ts**

```typescript
import * as readline from "node:readline";
import { parseInbound, sendMessage } from "./protocol.js";
import { createAgentState, hydrateHistory, createLoopConfig } from "./agent.js";
import { resolveToolCall } from "./tools.js";
import { agentLoop } from "@mariozechner/pi-agent-core";
import { findModel } from "@mariozechner/pi-ai";
import type { AgentEvent } from "@mariozechner/pi-agent-core";

const state = createAgentState();
let apiKey: string | null = null;
let currentModel: string | null = null;

const rl = readline.createInterface({ input: process.stdin });

rl.on("line", async (line: string) => {
  const msg = parseInbound(line);
  if (!msg) return;

  switch (msg.type) {
    case "shutdown":
      process.exit(0);
      break;

    case "history":
      hydrateHistory(state, msg.messages);
      break;

    case "set_config": {
      apiKey = msg.api_key;
      currentModel = msg.model;
      try {
        const model = findModel(msg.model);
        if (model) {
          state.model = model;
        }
        // Fetch available models from OpenRouter to validate key
        const response = await fetch("https://openrouter.ai/api/v1/models", {
          headers: { Authorization: `Bearer ${msg.api_key}` },
        });
        if (!response.ok) {
          sendMessage({ type: "error", message: "Invalid API key" });
          return;
        }
        const data = await response.json() as { data: { id: string; name: string }[] };
        sendMessage({
          type: "models",
          models: data.data.map((m: any) => ({ id: m.id, name: m.name || m.id })),
        });
      } catch (e) {
        sendMessage({ type: "error", message: String(e) });
      }
      break;
    }

    case "tool_result":
      resolveToolCall(msg.call_id, msg.content);
      break;

    case "user_message": {
      if (!apiKey || !state.model) {
        sendMessage({ type: "error", message: "Assistant not configured. Set API key and model first." });
        return;
      }

      // Add user message to state
      state.messages.push({
        role: "user",
        content: msg.content,
        timestamp: Date.now(),
      });

      try {
        const config = createLoopConfig(state, apiKey);
        let totalUsage = { input: 0, output: 0, cost: 0 };

        for await (const event of agentLoop(state, config) as AsyncIterable<AgentEvent>) {
          switch (event.type) {
            case "message_update":
              if (event.assistantMessageEvent.type === "text_delta") {
                sendMessage({ type: "chunk", text: event.assistantMessageEvent.delta });
              }
              break;
            case "turn_end":
              if ("usage" in event.message && event.message.role === "assistant") {
                const usage = (event.message as any).usage;
                if (usage) {
                  totalUsage.input += usage.input || 0;
                  totalUsage.output += usage.output || 0;
                  totalUsage.cost += usage.cost?.total || 0;
                }
              }
              break;
          }
        }

        sendMessage({
          type: "done",
          usage: {
            input_tokens: totalUsage.input,
            output_tokens: totalUsage.output,
            cost_usd: Math.round(totalUsage.cost * 10000) / 10000,
          },
        });
      } catch (e) {
        sendMessage({ type: "error", message: String(e) });
      }
      break;
    }
  }
});
```

- [ ] **Step 2: Verify TypeScript compiles**

```bash
cd sidecar && npx tsc --noEmit
```

- [ ] **Step 3: Commit**

```bash
git add sidecar/src/index.ts
git commit -m "feat(sidecar): implement main loop with stdin/stdout protocol"
```

---

### Task 7: Sidecar Build Script

**Files:**
- Create: `sidecar/build.sh`

- [ ] **Step 1: Create sidecar/build.sh**

```bash
#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BINARIES_DIR="$SCRIPT_DIR/../src-tauri/binaries"

mkdir -p "$BINARIES_DIR"

# Detect current platform and build only for it (dev mode)
# Cross-platform builds happen in CI
ARCH=$(uname -m)
OS=$(uname -s)

if [ "$OS" = "Linux" ]; then
  if [ "$ARCH" = "aarch64" ] || [ "$ARCH" = "arm64" ]; then
    TARGET="bun-linux-arm64"
    SUFFIX="aarch64-unknown-linux-gnu"
  else
    TARGET="bun-linux-x64"
    SUFFIX="x86_64-unknown-linux-gnu"
  fi
elif [ "$OS" = "Darwin" ]; then
  if [ "$ARCH" = "arm64" ]; then
    TARGET="bun-darwin-arm64"
    SUFFIX="aarch64-apple-darwin"
  else
    TARGET="bun-darwin-x64"
    SUFFIX="x86_64-apple-darwin"
  fi
else
  echo "Unsupported platform: $OS"
  exit 1
fi

echo "Building racc-assistant for $TARGET..."
cd "$SCRIPT_DIR"
bun build --compile --target="$TARGET" src/index.ts --outfile "$BINARIES_DIR/racc-assistant-$SUFFIX"
echo "Built: $BINARIES_DIR/racc-assistant-$SUFFIX"
```

- [ ] **Step 2: Make it executable**

```bash
chmod +x sidecar/build.sh
```

- [ ] **Step 3: Add binaries to .gitignore**

Add to the project root `.gitignore`:

```
src-tauri/binaries/racc-assistant-*
sidecar/node_modules/
sidecar/dist/
```

- [ ] **Step 4: Commit**

```bash
git add sidecar/build.sh .gitignore
git commit -m "feat(sidecar): add build script for platform-specific binary compilation"
```

---

## Chunk 3: Frontend

### Task 8: TypeScript Types and Zustand Store

**Files:**
- Create: `src/types/assistant.ts`
- Create: `src/stores/assistantStore.ts`

- [ ] **Step 1: Create src/types/assistant.ts**

```typescript
export interface AssistantMessage {
  id: number;
  role: "user" | "assistant" | "tool_call" | "tool_result";
  content: string;
  tool_name?: string;
  tool_call_id?: string;
  created_at: string;
}

export interface AssistantConfig {
  provider: string | null;
  api_key: string | null;
  model: string | null;
}

export interface ModelOption {
  id: string;
  name: string;
}
```

- [ ] **Step 2: Create src/stores/assistantStore.ts**

```typescript
import { create } from "zustand";
import { invoke } from "@tauri-apps/api/core";
import type { AssistantMessage, AssistantConfig, ModelOption } from "../types/assistant";

interface AssistantState {
  messages: AssistantMessage[];
  isStreaming: boolean;
  streamingText: string;
  config: AssistantConfig | null;
  models: ModelOption[];
  assistantCost: number;
  error: string | null;

  loadConfig: () => Promise<void>;
  saveConfig: (provider: string, apiKey: string, model: string) => Promise<void>;
  loadHistory: () => Promise<void>;
  sendMessage: (content: string) => Promise<void>;
  appendChunk: (text: string) => void;
  finishStreaming: (usage: { input_tokens: number; output_tokens: number; cost_usd: number }) => void;
  setModels: (models: ModelOption[]) => void;
  setError: (error: string | null) => void;
  clearError: () => void;
}

export const useAssistantStore = create<AssistantState>((set, get) => ({
  messages: [],
  isStreaming: false,
  streamingText: "",
  config: null,
  models: [],
  assistantCost: 0,
  error: null,

  loadConfig: async () => {
    try {
      const config = await invoke<AssistantConfig>("get_assistant_config");
      set({ config });
    } catch (e) {
      set({ error: String(e) });
    }
  },

  saveConfig: async (provider, apiKey, model) => {
    try {
      await invoke("set_assistant_config", { provider, apiKey, model });
      set({ config: { provider, api_key: apiKey, model } });
    } catch (e) {
      set({ error: String(e) });
    }
  },

  loadHistory: async () => {
    try {
      const messages = await invoke<AssistantMessage[]>("get_assistant_messages", { limit: 50 });
      set({ messages });
    } catch (e) {
      set({ error: String(e) });
    }
  },

  sendMessage: async (content) => {
    const userMsg: AssistantMessage = {
      id: Date.now(),
      role: "user",
      content,
      created_at: new Date().toISOString(),
    };

    set((s) => ({
      messages: [...s.messages, userMsg],
      isStreaming: true,
      streamingText: "",
      error: null,
    }));

    try {
      // Persist user message
      await invoke("save_assistant_message", {
        role: "user",
        content,
        toolName: null,
        toolCallId: null,
      });

      // Send to sidecar via Rust backend
      await invoke("assistant_send_message", { content });
    } catch (e) {
      set({ isStreaming: false, error: String(e) });
    }
  },

  appendChunk: (text) => {
    set((s) => ({ streamingText: s.streamingText + text }));
  },

  finishStreaming: (usage) => {
    const { streamingText } = get();
    if (streamingText) {
      const assistantMsg: AssistantMessage = {
        id: Date.now(),
        role: "assistant",
        content: streamingText,
        created_at: new Date().toISOString(),
      };
      set((s) => ({
        messages: [...s.messages, assistantMsg],
        isStreaming: false,
        streamingText: "",
        assistantCost: s.assistantCost + usage.cost_usd,
      }));

      // Persist assistant message (fire-and-forget)
      invoke("save_assistant_message", {
        role: "assistant",
        content: streamingText,
        toolName: null,
        toolCallId: null,
      }).catch(() => {});
    } else {
      set({ isStreaming: false, streamingText: "" });
    }
  },

  setModels: (models) => set({ models }),
  setError: (error) => set({ error }),
  clearError: () => set({ error: null }),
}));
```

- [ ] **Step 3: Verify TypeScript compiles**

```bash
./node_modules/.bin/tsc --noEmit
```

- [ ] **Step 4: Commit**

```bash
git add src/types/assistant.ts src/stores/assistantStore.ts
git commit -m "feat(assistant): add TypeScript types and Zustand store"
```

---

### Task 9: AssistantMessage and AssistantChat Components

**Files:**
- Create: `src/components/Assistant/AssistantMessage.tsx`
- Create: `src/components/Assistant/AssistantChat.tsx`
- Modify: `package.json` (add react-markdown)

- [ ] **Step 1: Install react-markdown and @tailwindcss/typography**

```bash
bun add react-markdown @tailwindcss/typography
```

- [ ] **Step 1b: Register typography plugin in tailwind.config.ts**

In `tailwind.config.ts`, add the import at the top and the plugin:

```typescript
import typography from "@tailwindcss/typography";
```

And in the `plugins` array:

```typescript
plugins: [typography],
```

- [ ] **Step 2: Create AssistantMessage.tsx**

```tsx
import Markdown from "react-markdown";

interface Props {
  role: "user" | "assistant" | "tool_call" | "tool_result";
  content: string;
}

export function AssistantMessage({ role, content }: Props) {
  if (role === "tool_call" || role === "tool_result") return null;

  const isUser = role === "user";

  return (
    <div className={`flex ${isUser ? "justify-end" : "justify-start"}`}>
      <div
        className={`max-w-[90%] rounded-lg px-3 py-2 text-xs ${
          isUser
            ? "bg-accent/20 text-zinc-200"
            : "bg-surface-2 text-zinc-300"
        }`}
      >
        {isUser ? (
          <p className="whitespace-pre-wrap">{content}</p>
        ) : (
          <div className="prose prose-invert prose-sm max-w-none [&_pre]:bg-surface-0 [&_pre]:p-2 [&_pre]:rounded [&_code]:text-[11px] [&_p]:my-1 [&_h2]:text-xs [&_h2]:mt-2 [&_h2]:mb-1 [&_h3]:text-xs [&_h3]:mt-2 [&_h3]:mb-1 [&_ul]:my-1 [&_li]:my-0">
            <Markdown>{content}</Markdown>
          </div>
        )}
      </div>
    </div>
  );
}
```

- [ ] **Step 3: Create AssistantChat.tsx**

```tsx
import { useState, useRef, useEffect } from "react";
import { useAssistantStore } from "../../stores/assistantStore";
import { useShallow } from "zustand/react/shallow";
import { AssistantMessage } from "./AssistantMessage";
import Markdown from "react-markdown";

export function AssistantChat() {
  const { messages, isStreaming, streamingText, sendMessage } = useAssistantStore(
    useShallow((s) => ({
      messages: s.messages,
      isStreaming: s.isStreaming,
      streamingText: s.streamingText,
      sendMessage: s.sendMessage,
    }))
  );
  const [input, setInput] = useState("");
  const messagesEndRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    messagesEndRef.current?.scrollIntoView({ behavior: "smooth" });
  }, [messages, streamingText]);

  const handleSend = () => {
    const trimmed = input.trim();
    if (!trimmed || isStreaming) return;
    setInput("");
    sendMessage(trimmed);
  };

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === "Enter" && !e.shiftKey) {
      e.preventDefault();
      handleSend();
    }
  };

  const quickActions = [
    { label: "Summarize Diff", prompt: "Summarize what my agents have changed. Show me a risk-prioritized overview." },
    { label: "Costs", prompt: "What are the current costs across all my sessions?" },
  ];

  return (
    <div className="flex flex-1 flex-col overflow-hidden">
      {/* Message list */}
      <div className="flex-1 overflow-y-auto p-3 space-y-3">
        {messages.length === 0 && !isStreaming && (
          <div className="flex items-center justify-center py-8 text-xs text-zinc-600">
            Ask me about your agents' work.
          </div>
        )}

        {messages.map((msg) => (
          <AssistantMessage key={msg.id} role={msg.role} content={msg.content} />
        ))}

        {/* Streaming message */}
        {isStreaming && streamingText && (
          <div className="flex justify-start">
            <div className="max-w-[90%] rounded-lg bg-surface-2 px-3 py-2 text-xs text-zinc-300">
              <div className="prose prose-invert prose-sm max-w-none [&_pre]:bg-surface-0 [&_pre]:p-2 [&_pre]:rounded [&_code]:text-[11px] [&_p]:my-1 [&_h2]:text-xs [&_h2]:mt-2 [&_h2]:mb-1 [&_h3]:text-xs [&_h3]:mt-2 [&_h3]:mb-1 [&_ul]:my-1 [&_li]:my-0">
                <Markdown>{streamingText}</Markdown>
              </div>
            </div>
          </div>
        )}

        {isStreaming && !streamingText && (
          <div className="flex justify-start">
            <div className="rounded-lg bg-surface-2 px-3 py-2 text-xs text-zinc-500">
              Thinking...
            </div>
          </div>
        )}

        <div ref={messagesEndRef} />
      </div>

      {/* Quick actions + Input */}
      <div className="border-t border-surface-3 p-2">
        <div className="mb-2 flex gap-1">
          {quickActions.map((action) => (
            <button
              key={action.label}
              onClick={() => {
                if (!isStreaming) sendMessage(action.prompt);
              }}
              disabled={isStreaming}
              className="rounded bg-surface-2 px-2 py-1 text-[10px] text-zinc-400 transition-colors duration-150 hover:bg-surface-3 hover:text-zinc-300 disabled:opacity-50"
            >
              {action.label}
            </button>
          ))}
        </div>
        <div className="flex gap-2">
          <input
            type="text"
            value={input}
            onChange={(e) => setInput(e.target.value)}
            onKeyDown={handleKeyDown}
            placeholder="Ask about your agents..."
            disabled={isStreaming}
            className="flex-1 rounded border border-surface-3 bg-surface-0 px-2 py-1.5 text-xs text-zinc-300 placeholder-zinc-600 outline-none focus:border-accent disabled:opacity-50"
          />
          <button
            onClick={handleSend}
            disabled={isStreaming || !input.trim()}
            className="rounded bg-accent px-3 py-1.5 text-xs font-medium text-white transition-colors duration-150 hover:bg-accent-hover disabled:opacity-50"
          >
            Send
          </button>
        </div>
      </div>
    </div>
  );
}
```

- [ ] **Step 4: Verify TypeScript compiles**

```bash
./node_modules/.bin/tsc --noEmit
```

- [ ] **Step 5: Commit**

```bash
git add src/components/Assistant/AssistantMessage.tsx src/components/Assistant/AssistantChat.tsx package.json bun.lockb
git commit -m "feat(assistant): add chat UI components with markdown rendering"
```

---

### Task 10: AssistantSetup and AssistantPanel Components

**Files:**
- Create: `src/components/Assistant/AssistantSetup.tsx`
- Create: `src/components/Assistant/AssistantPanel.tsx`

- [ ] **Step 1: Create AssistantSetup.tsx**

```tsx
import { useState } from "react";
import { useAssistantStore } from "../../stores/assistantStore";
import { useShallow } from "zustand/react/shallow";

export function AssistantSetup() {
  const { saveConfig, models, setModels, error, setError } = useAssistantStore(
    useShallow((s) => ({
      saveConfig: s.saveConfig,
      models: s.models,
      setModels: s.setModels,
      error: s.error,
      setError: s.setError,
    }))
  );

  const [apiKey, setApiKey] = useState("");
  const [selectedModel, setSelectedModel] = useState("");
  const [loadingModels, setLoadingModels] = useState(false);

  const fetchModels = async () => {
    if (!apiKey.trim()) return;
    setLoadingModels(true);
    setError(null);

    try {
      const response = await fetch("https://openrouter.ai/api/v1/models", {
        headers: { Authorization: `Bearer ${apiKey}` },
      });

      if (!response.ok) {
        setError("Invalid API key");
        setLoadingModels(false);
        return;
      }

      const data = await response.json();
      const modelList = (data.data || [])
        .map((m: any) => ({ id: m.id, name: m.name || m.id }))
        .sort((a: any, b: any) => a.name.localeCompare(b.name));

      setModels(modelList);
      // Default to Sonnet if available
      const defaultModel = modelList.find((m: any) => m.id.includes("claude-sonnet")) || modelList[0];
      if (defaultModel) setSelectedModel(defaultModel.id);
    } catch (e) {
      setError(String(e));
    } finally {
      setLoadingModels(false);
    }
  };

  const handleSave = async () => {
    if (!apiKey.trim() || !selectedModel) return;
    await saveConfig("openrouter", apiKey, selectedModel);
  };

  return (
    <div className="flex flex-1 flex-col items-center justify-center p-4">
      <div className="w-full max-w-xs space-y-3">
        <h3 className="text-xs font-semibold uppercase tracking-wider text-zinc-400">
          Assistant Setup
        </h3>

        <div>
          <label className="mb-1 block text-[10px] text-zinc-500">Provider</label>
          <select
            className="w-full rounded border border-surface-3 bg-surface-0 px-2 py-1.5 text-xs text-zinc-300 outline-none focus:border-accent"
            value="openrouter"
            disabled
          >
            <option value="openrouter">OpenRouter</option>
          </select>
        </div>

        <div>
          <label className="mb-1 block text-[10px] text-zinc-500">API Key</label>
          <input
            type="password"
            value={apiKey}
            onChange={(e) => setApiKey(e.target.value)}
            onBlur={fetchModels}
            onKeyDown={(e) => e.key === "Enter" && fetchModels()}
            placeholder="sk-or-..."
            className="w-full rounded border border-surface-3 bg-surface-0 px-2 py-1.5 text-xs text-zinc-300 placeholder-zinc-600 outline-none focus:border-accent"
          />
        </div>

        <div>
          <label className="mb-1 block text-[10px] text-zinc-500">Model</label>
          <select
            value={selectedModel}
            onChange={(e) => setSelectedModel(e.target.value)}
            disabled={models.length === 0}
            className="w-full rounded border border-surface-3 bg-surface-0 px-2 py-1.5 text-xs text-zinc-300 outline-none focus:border-accent disabled:opacity-50"
          >
            {models.length === 0 ? (
              <option>{loadingModels ? "Loading models..." : "Enter API key first"}</option>
            ) : (
              models.map((m) => (
                <option key={m.id} value={m.id}>
                  {m.name}
                </option>
              ))
            )}
          </select>
        </div>

        {error && (
          <p className="rounded bg-red-500/10 px-2 py-1 text-[10px] text-red-400">
            {error}
          </p>
        )}

        <button
          onClick={handleSave}
          disabled={!apiKey.trim() || !selectedModel}
          className="w-full rounded bg-accent px-3 py-1.5 text-xs font-medium text-white transition-colors duration-150 hover:bg-accent-hover disabled:opacity-50"
        >
          Save
        </button>
      </div>
    </div>
  );
}
```

- [ ] **Step 2: Create AssistantPanel.tsx**

```tsx
import { useEffect } from "react";
import { useAssistantStore } from "../../stores/assistantStore";
import { useShallow } from "zustand/react/shallow";
import { AssistantSetup } from "./AssistantSetup";
import { AssistantChat } from "./AssistantChat";

export function AssistantPanel() {
  const { config, assistantCost, loadConfig, loadHistory } = useAssistantStore(
    useShallow((s) => ({
      config: s.config,
      assistantCost: s.assistantCost,
      loadConfig: s.loadConfig,
      loadHistory: s.loadHistory,
    }))
  );

  useEffect(() => {
    loadConfig();
    loadHistory();
  }, [loadConfig, loadHistory]);

  const isConfigured = config?.api_key && config?.model;

  return (
    <div className="flex flex-1 flex-col overflow-hidden border-t border-surface-3">
      <div className="flex items-center justify-between border-b border-surface-3 px-4 py-2">
        <h2 className="text-xs font-semibold uppercase tracking-wider text-zinc-400">
          Assistant
        </h2>
        {isConfigured && assistantCost > 0 && (
          <span className="text-[10px] text-zinc-600">
            ${assistantCost.toFixed(4)}
          </span>
        )}
      </div>

      {isConfigured ? <AssistantChat /> : <AssistantSetup />}
    </div>
  );
}
```

- [ ] **Step 3: Verify TypeScript compiles**

```bash
./node_modules/.bin/tsc --noEmit
```

- [ ] **Step 4: Commit**

```bash
git add src/components/Assistant/AssistantSetup.tsx src/components/Assistant/AssistantPanel.tsx
git commit -m "feat(assistant): add setup and panel container components"
```

---

### Task 11: Wire Into App Layout

**Files:**
- Modify: `src/App.tsx`
- Remove: `src/components/ActivityLog/ActivityLog.tsx`

- [ ] **Step 1: Update App.tsx to use AssistantPanel**

Replace the ActivityLog import and usage:

In `src/App.tsx`, change:
```tsx
import { ActivityLog } from "./components/ActivityLog/ActivityLog";
```
to:
```tsx
import { AssistantPanel } from "./components/Assistant/AssistantPanel";
```

And replace:
```tsx
<ActivityLog />
```
with:
```tsx
<AssistantPanel />
```

- [ ] **Step 2: Delete ActivityLog**

```bash
rm src/components/ActivityLog/ActivityLog.tsx
rmdir src/components/ActivityLog
```

- [ ] **Step 3: Verify TypeScript compiles**

```bash
./node_modules/.bin/tsc --noEmit
```

- [ ] **Step 4: Verify Vite builds**

```bash
./node_modules/.bin/vite build
```

Expected: builds successfully

- [ ] **Step 5: Commit**

```bash
git add src/App.tsx src/components/Assistant/
git rm src/components/ActivityLog/ActivityLog.tsx
git commit -m "feat(assistant): wire AssistantPanel into app layout, remove ActivityLog"
```

---

## Chunk 4: Integration

### Task 12: Rust Sidecar Process Management

This task adds the Rust command that spawns and communicates with the sidecar binary. This is the glue between the frontend's `sendMessage()` and the sidecar's stdin/stdout protocol.

**Files:**
- Modify: `src-tauri/src/commands/assistant.rs`
- Modify: `src-tauri/src/lib.rs`

- [ ] **Step 1: Add tokio process and IO dependencies to Cargo.toml**

In `src-tauri/Cargo.toml`, ensure `tokio` has the `process` and `io-util` features:

```toml
tokio = { version = "1", features = ["full"] }
```

> **Note:** Tauri already depends on tokio, but we need `process` and `io-util` features for async child process management. If Tauri's tokio dep doesn't include these, add tokio explicitly.

- [ ] **Step 2: Add sidecar process management to assistant.rs**

Add these imports and the sidecar management code to `src-tauri/src/commands/assistant.rs`:

```rust
use std::io::Write;
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, BufReader as TokioBufReader};
use tokio::process::{Child, ChildStdin, ChildStdout, Command as TokioCommand};
use tauri::Manager;

// Sidecar state — uses tokio::sync::Mutex to safely hold across .await points
pub struct SidecarState {
    pub child: Option<Child>,
    pub stdin: Option<std::process::ChildStdin>,
    pub reader: Option<TokioBufReader<ChildStdout>>,
}

impl SidecarState {
    pub fn new() -> Self {
        Self { child: None, stdin: None, reader: None }
    }
}

fn resolve_sidecar_path(app: &tauri::AppHandle) -> Result<std::path::PathBuf, String> {
    // Determine platform triple suffix
    let suffix = if cfg!(target_os = "linux") && cfg!(target_arch = "x86_64") {
        "x86_64-unknown-linux-gnu"
    } else if cfg!(target_os = "linux") && cfg!(target_arch = "aarch64") {
        "aarch64-unknown-linux-gnu"
    } else if cfg!(target_os = "macos") && cfg!(target_arch = "aarch64") {
        "aarch64-apple-darwin"
    } else if cfg!(target_os = "macos") && cfg!(target_arch = "x86_64") {
        "x86_64-apple-darwin"
    } else if cfg!(target_os = "windows") {
        "x86_64-pc-windows-msvc"
    } else {
        return Err("Unsupported platform".to_string());
    };

    let binary_name = format!("racc-assistant-{suffix}");

    // Production: check Tauri resource dir
    if let Ok(resource_dir) = app.path().resource_dir() {
        let path = resource_dir.join("binaries").join(&binary_name);
        if path.exists() {
            return Ok(path);
        }
    }

    // Development: check src-tauri/binaries (Tauri sets CWD to src-tauri during dev)
    let dev_path = std::path::PathBuf::from("binaries").join(&binary_name);
    if dev_path.exists() {
        return Ok(dev_path);
    }

    // Development fallback: check from project root
    let project_path = std::path::PathBuf::from("src-tauri/binaries").join(&binary_name);
    if project_path.exists() {
        return Ok(project_path);
    }

    Err(format!(
        "Sidecar binary '{binary_name}' not found. Run sidecar/build.sh first."
    ))
}

fn spawn_sidecar(app: &tauri::AppHandle) -> Result<(Child, std::process::ChildStdin, TokioBufReader<ChildStdout>), String> {
    let path = resolve_sidecar_path(app)?;

    let mut child = TokioCommand::new(path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("Failed to spawn sidecar: {e}"))?;

    // Take ownership of stdin and stdout separately to avoid borrow conflicts
    let stdout = child.stdout.take()
        .ok_or("Failed to capture sidecar stdout")?;
    let stdin = child.stdin.take()
        .ok_or("Failed to capture sidecar stdin")?;

    // Convert tokio ChildStdin to std ChildStdin for synchronous writes
    let std_stdin = stdin.into_std().map_err(|e| format!("Failed to convert stdin: {e}"))?;
    let reader = TokioBufReader::new(stdout);

    Ok((child, std_stdin, reader))
}

fn write_to_stdin(stdin: &mut std::process::ChildStdin, msg: &str) -> Result<(), String> {
    writeln!(stdin, "{}", msg).map_err(|e| format!("Failed to write to sidecar: {e}"))?;
    stdin.flush().map_err(|e| format!("Failed to flush sidecar stdin: {e}"))?;
    Ok(())
}

#[tauri::command]
pub async fn assistant_send_message(
    app: tauri::AppHandle,
    sidecar: tauri::State<'_, tokio::sync::Mutex<SidecarState>>,
    db: tauri::State<'_, Mutex<Connection>>,
    content: String,
) -> Result<(), String> {
    let mut sidecar_state = sidecar.lock().await;

    // Lazy spawn
    if sidecar_state.child.is_none() {
        let (child, mut stdin, reader) = spawn_sidecar(&app)?;

        // Send config if available (read DB before holding sidecar lock long)
        let config = {
            let conn = db.lock().map_err(|e| e.to_string())?;
            let get_val = |key: &str| -> Option<String> {
                conn.query_row(
                    "SELECT value FROM assistant_config WHERE key = ?1",
                    [key],
                    |row| row.get(0),
                )
                .ok()
            };
            (get_val("provider"), get_val("api_key"), get_val("model"))
        };

        if let (Some(provider), Some(api_key), Some(model)) = config {
            let config_msg = serde_json::json!({
                "type": "set_config",
                "provider": provider,
                "api_key": api_key,
                "model": model
            });
            write_to_stdin(&mut stdin, &config_msg.to_string())?;
        }

        // Send history
        let history = {
            let conn = db.lock().map_err(|e| e.to_string())?;
            let mut stmt = conn
                .prepare(
                    "SELECT role, content, tool_name, tool_call_id FROM assistant_messages ORDER BY id DESC LIMIT 50",
                )
                .map_err(|e| e.to_string())?;

            let msgs: Vec<serde_json::Value> = stmt
                .query_map([], |row| {
                    Ok(serde_json::json!({
                        "role": row.get::<_, String>(0)?,
                        "content": row.get::<_, String>(1)?,
                        "tool_name": row.get::<_, Option<String>>(2)?,
                        "tool_call_id": row.get::<_, Option<String>>(3)?
                    }))
                })
                .map_err(|e| e.to_string())?
                .filter_map(|r| r.ok())
                .collect();

            let mut msgs = msgs;
            msgs.reverse();
            msgs
        };

        let history_msg = serde_json::json!({
            "type": "history",
            "messages": history
        });
        write_to_stdin(&mut stdin, &history_msg.to_string())?;

        sidecar_state.child = Some(child);
        sidecar_state.stdin = Some(stdin);
        sidecar_state.reader = Some(reader);
    }

    // Send user message
    let msg = serde_json::json!({
        "type": "user_message",
        "content": content
    });

    if let Some(stdin) = sidecar_state.stdin.as_mut() {
        write_to_stdin(stdin, &msg.to_string())?;
    }

    Ok(())
}

#[tauri::command]
pub async fn assistant_read_response(
    sidecar: tauri::State<'_, tokio::sync::Mutex<SidecarState>>,
    db: tauri::State<'_, Mutex<Connection>>,
) -> Result<String, String> {
    let mut sidecar_state = sidecar.lock().await;

    let reader = sidecar_state.reader.as_mut()
        .ok_or("Sidecar not running")?;

    // Read one line asynchronously (does not block the tokio runtime)
    let mut line = String::new();
    reader.read_line(&mut line).await
        .map_err(|e| format!("Failed to read from sidecar: {e}"))?;

    if line.is_empty() {
        return Err("Sidecar process exited".to_string());
    }

    // Handle tool calls in a loop — LLM may issue multiple sequential tool calls
    loop {
        let parsed = match serde_json::from_str::<serde_json::Value>(line.trim()) {
            Ok(v) => v,
            Err(_) => return Ok(line.trim().to_string()),
        };

        if parsed.get("type").and_then(|t| t.as_str()) != Some("tool_call") {
            return Ok(line.trim().to_string());
        }

        // It's a tool call — resolve it
        let tool_name = parsed["name"].as_str().unwrap_or("");
        let tool_id = parsed["id"].as_str().unwrap_or("");
        let args = &parsed["args"];

        let result = match tool_name {
            "get_all_sessions" => {
                // Reuse the existing get_all_sessions_for_assistant command logic
                let conn = db.lock().map_err(|e| e.to_string())?;
                let mut stmt = conn
                    .prepare(
                        "SELECT s.id, s.status, s.agent, s.branch, r.name, r.path, s.worktree_path, s.created_at
                         FROM sessions s JOIN repos r ON s.repo_id = r.id ORDER BY s.created_at DESC",
                    )
                    .map_err(|e| e.to_string())?;

                let now = chrono::Utc::now();
                let sessions: Vec<serde_json::Value> = stmt
                    .query_map([], |row| {
                        let created_at: String = row.get(7)?;
                        let elapsed = chrono::NaiveDateTime::parse_from_str(&created_at, "%Y-%m-%d %H:%M:%S")
                            .map(|ndt| (now - ndt.and_utc()).num_minutes())
                            .unwrap_or(0);
                        Ok(serde_json::json!({
                            "id": row.get::<_, i64>(0)?,
                            "status": row.get::<_, String>(1)?,
                            "agent": row.get::<_, String>(2)?,
                            "branch": row.get::<_, Option<String>>(3)?,
                            "repo_name": row.get::<_, String>(4)?,
                            "repo_path": row.get::<_, String>(5)?,
                            "worktree_path": row.get::<_, Option<String>>(6)?,
                            "elapsed_minutes": elapsed,
                            "created_at": created_at
                        }))
                    })
                    .map_err(|e| e.to_string())?
                    .filter_map(|r| r.ok())
                    .collect();

                serde_json::to_string(&sessions).unwrap_or_default()
            }
            "get_session_diff" => {
                let session_id = args["session_id"].as_i64().unwrap_or(0);
                let path = {
                    let conn = db.lock().map_err(|e| e.to_string())?;
                    resolve_session_path(&conn, session_id)?
                };
                // Use tokio::process for non-blocking git diff
                let output = tokio::process::Command::new("git")
                    .args(["diff", "HEAD"])
                    .current_dir(&path)
                    .output()
                    .await
                    .map_err(|e| format!("Failed to get diff: {e}"))?;
                String::from_utf8_lossy(&output.stdout).to_string()
            }
            "get_session_costs" => {
                let session_id = args["session_id"].as_i64().unwrap_or(0);
                let path = {
                    let conn = db.lock().map_err(|e| e.to_string())?;
                    resolve_session_path(&conn, session_id)?
                };
                let costs = crate::commands::cost::get_project_costs(path).await
                    .unwrap_or_default();
                serde_json::to_string(&costs).unwrap_or_default()
            }
            _ => "Unknown tool".to_string(),
        };

        // Send tool result back to sidecar
        let tool_result = serde_json::json!({
            "type": "tool_result",
            "call_id": tool_id,
            "content": result
        });
        if let Some(stdin) = sidecar_state.stdin.as_mut() {
            write_to_stdin(stdin, &tool_result.to_string())?;
        }

        // Read the next line — may be another tool call or a chunk/done/error
        line.clear();
        reader.read_line(&mut line).await
            .map_err(|e| format!("Failed to read from sidecar: {e}"))?;

        if line.is_empty() {
            return Err("Sidecar process exited".to_string());
        }

        // Loop continues to check if this is another tool_call
    }
}

#[tauri::command]
pub async fn assistant_shutdown(
    sidecar: tauri::State<'_, tokio::sync::Mutex<SidecarState>>,
) -> Result<(), String> {
    let mut state = sidecar.lock().await;
    if let Some(mut stdin) = state.stdin.take() {
        let shutdown_msg = serde_json::json!({"type": "shutdown"});
        write_to_stdin(&mut stdin, &shutdown_msg.to_string()).ok();
    }
    if let Some(mut child) = state.child.take() {
        child.kill().await.ok();
    }
    state.reader = None;
    Ok(())
}
```

> **Key design decisions vs. original plan:**
> - Uses `tokio::process::Command` instead of `std::process::Command` — async I/O prevents blocking the tokio runtime.
> - Uses `tokio::sync::Mutex` for `SidecarState` instead of `std::sync::Mutex` — safe to hold across `.await` points.
> - Stdin and stdout are taken from the `Child` at spawn time and stored separately — avoids borrow checker conflicts.
> - `BufReader` is created once at spawn and reused — prevents loss of buffered data.
> - Tool call handling loops until a non-tool-call message arrives — supports multiple sequential tool calls.
> - `elapsed_minutes` is computed in the tool call relay (matching Task 2's pattern).
> - `resolve_sidecar_path` uses platform-specific triple suffix matching Tauri's `externalBin` convention.

- [ ] **Step 3: Register sidecar state and new commands in lib.rs**

Update `src-tauri/src/lib.rs`:

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
        .manage(tokio::sync::Mutex::new(commands::assistant::SidecarState::new()))
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
            commands::assistant::get_assistant_config,
            commands::assistant::set_assistant_config,
            commands::assistant::save_assistant_message,
            commands::assistant::get_assistant_messages,
            commands::assistant::get_all_sessions_for_assistant,
            commands::assistant::get_session_diff_for_assistant,
            commands::assistant::get_session_costs_for_assistant,
            commands::assistant::assistant_send_message,
            commands::assistant::assistant_read_response,
            commands::assistant::assistant_shutdown,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
```

- [ ] **Step 4: Add Default impl for ProjectCosts in cost.rs**

In `src-tauri/src/commands/cost.rs`, add `Default` to the ProjectCosts derive:

```rust
#[derive(Debug, Clone, Serialize, Default)]
pub struct ProjectCosts {
```

- [ ] **Step 5: Verify Rust compiles**

```bash
cd src-tauri && cargo check
```

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/commands/assistant.rs src-tauri/src/lib.rs src-tauri/src/commands/cost.rs src-tauri/Cargo.toml
git commit -m "feat(assistant): add sidecar process management and tool call relay"
```

---

### Task 13: Frontend Sidecar Communication

This task adds the frontend polling loop that reads responses from the sidecar via the Rust backend and updates the assistant store.

**Files:**
- Modify: `src/stores/assistantStore.ts`

- [ ] **Step 1: Add response polling to the sendMessage action**

Update the `sendMessage` action in `src/stores/assistantStore.ts` — after the `invoke("assistant_send_message")` call, add a polling loop:

```typescript
sendMessage: async (content) => {
    const userMsg: AssistantMessage = {
      id: Date.now(),
      role: "user",
      content,
      created_at: new Date().toISOString(),
    };

    set((s) => ({
      messages: [...s.messages, userMsg],
      isStreaming: true,
      streamingText: "",
      error: null,
    }));

    try {
      await invoke("save_assistant_message", {
        role: "user",
        content,
        toolName: null,
        toolCallId: null,
      });

      await invoke("assistant_send_message", { content });

      // Poll for responses
      let done = false;
      while (!done) {
        try {
          // assistant_read_response uses async I/O — it awaits the next
          // sidecar output line and handles tool calls internally (looping
          // until a non-tool-call message is ready), so this is not a busy poll.
          const line = await invoke<string>("assistant_read_response");
          if (!line) {
            // Empty response — add a small delay before retrying
            await new Promise((r) => setTimeout(r, 50));
            continue;
          }

          const msg = JSON.parse(line);
          switch (msg.type) {
            case "chunk":
              get().appendChunk(msg.text);
              break;
            case "done":
              get().finishStreaming(msg.usage || { input_tokens: 0, output_tokens: 0, cost_usd: 0 });
              done = true;
              break;
            case "error":
              set({ isStreaming: false, error: msg.message });
              done = true;
              break;
            case "models":
              set({ models: msg.models });
              break;
            default:
              // Unknown message type (e.g. tool_call that wasn't handled server-side) — skip
              break;
          }
        } catch {
          set({ isStreaming: false, error: "Lost connection to assistant" });
          done = true;
        }
      }
    } catch (e) {
      set({ isStreaming: false, error: String(e) });
    }
  },
```

- [ ] **Step 2: Verify TypeScript compiles**

```bash
./node_modules/.bin/tsc --noEmit
```

- [ ] **Step 3: Commit**

```bash
git add src/stores/assistantStore.ts
git commit -m "feat(assistant): add response polling loop for sidecar communication"
```

---

### Task 14: Build Sidecar and End-to-End Verification

- [ ] **Step 1: Build the sidecar binary**

```bash
cd sidecar && bun install && bash build.sh
```

Expected: binary created at `src-tauri/binaries/racc-assistant-<triple>`

- [ ] **Step 2: Verify full Rust compilation**

```bash
cd src-tauri && cargo build
```

- [ ] **Step 3: Verify full frontend compilation**

```bash
./node_modules/.bin/tsc --noEmit && ./node_modules/.bin/vite build
```

- [ ] **Step 4: Manual smoke test**

```bash
bun tauri dev
```

Verify:
1. App launches without errors
2. Right panel shows "Assistant" header with setup form
3. Entering an OpenRouter API key fetches model list
4. After saving config, chat input appears
5. Sending "what sessions do I have?" gets a response (if sessions exist)

- [ ] **Step 5: Final commit**

```bash
git add src/ src-tauri/ sidecar/ package.json bun.lockb tailwind.config.ts .gitignore
git commit -m "feat(assistant): complete v1 integration — sidecar, backend, frontend"
```
