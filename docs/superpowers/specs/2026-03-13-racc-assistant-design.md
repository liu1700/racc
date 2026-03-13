# Racc Assistant — Global AI Butler for Multi-Agent Management

**Date:** 2026-03-13
**Status:** Approved
**Scope:** v1 MVP — diff summary & risk triage

## Problem

When coding agents change dozens or hundreds of files, developers don't review them. A traditional diff viewer doesn't solve this — the volume itself is the problem. Developers need an intelligent layer that triages changes, highlights risks, and lets them ask follow-up questions.

## Solution

A global AI assistant ("butler") that lives in Racc's right panel, powered by pi-ai and pi-agent-core running as a Tauri sidecar. It maintains one continuous conversation, sees all sessions, and can analyze any session's diffs on demand.

v1 capability: diff summary & risk triage only. Future capabilities (session narration, cross-session queries, review guidance) will be added incrementally.

**Tradeoff — Activity Log:** This assistant replaces the previously planned Activity Log (P0 feature in the wiki). The activity log's original purpose — showing which files agents read, which commands they ran — is partially addressed by the assistant's ability to summarize diffs. Structured event tracking is deferred; the assistant provides higher-value intelligence over raw event lists.

## Architecture

### Sidecar (Bun-compiled binary)

The assistant runs as a Tauri sidecar process — a standalone TypeScript binary compiled with `bun build --compile`. It uses:

- `@mariozechner/pi-ai` — unified LLM API (OpenRouter provider for v1)
- `@mariozechner/pi-agent-core` — agent runtime with tool calling and state management

```
Frontend (React)
  │ invoke("assistant_send", { message })
  ▼
Rust Backend (assistant.rs)
  │ spawn/communicate with sidecar
  │ persist messages to SQLite
  ▼ stdin/stdout JSON lines
Sidecar Binary (pi-ai + pi-agent-core)
  │ tool calls relayed back to Rust
  │ LLM responses streamed to frontend
  ▼ HTTPS
OpenRouter → Anthropic/OpenAI/Google/etc.
```

### Tauri Sidecar Configuration

The sidecar binary must be registered with Tauri for bundling and permissions.

**`tauri.conf.json` — add `externalBin`:**

```json
{
  "bundle": {
    "externalBin": ["binaries/racc-assistant"]
  }
}
```

Tauri requires platform-specific binary names following the target triple convention:
- `binaries/racc-assistant-x86_64-unknown-linux-gnu`
- `binaries/racc-assistant-aarch64-apple-darwin`
- `binaries/racc-assistant-x86_64-pc-windows-msvc.exe`

These are produced by `bun build --compile --target=<platform>` during the build process.

**`capabilities/default.json` — update permissions:**

```json
{
  "description": "Default permissions for Racc",
  "permissions": [
    "core:default",
    "shell:allow-open",
    "shell:allow-execute",
    "dialog:default",
    "dialog:allow-open",
    "pty:default"
  ]
}
```

Note: `shell:allow-execute` covers sidecar spawning. For production, consider scoping to only the `racc-assistant` binary via Tauri's scoped shell commands.

**Cross-platform build pipeline:**

The `sidecar/` project includes a build script that compiles for all target platforms:

```bash
# sidecar/build.sh (called by tauri build pipeline)
bun build --compile --target=bun-linux-x64 src/index.ts --outfile ../src-tauri/binaries/racc-assistant-x86_64-unknown-linux-gnu
bun build --compile --target=bun-darwin-arm64 src/index.ts --outfile ../src-tauri/binaries/racc-assistant-aarch64-apple-darwin
bun build --compile --target=bun-windows-x64 src/index.ts --outfile ../src-tauri/binaries/racc-assistant-x86_64-pc-windows-msvc.exe
```

For development, only the current platform binary is needed.

### Message Protocol (JSON lines over stdin/stdout)

**Frontend → Rust → Sidecar:**

```json
{"type":"user_message","content":"what did my agents change?"}
{"type":"tool_result","call_id":"x","content":"diff --git a/..."}
{"type":"set_config","provider":"openrouter","api_key":"sk-or-...","model":"anthropic/claude-sonnet-4"}
{"type":"history","messages":[...]}
{"type":"shutdown"}
```

**Sidecar → Rust → Frontend:**

```json
{"type":"chunk","text":"## Summary\n..."}
{"type":"tool_call","id":"x","name":"get_session_diff","args":{"session_id":1}}
{"type":"done","usage":{"input_tokens":1234,"output_tokens":567,"cost_usd":0.03}}
{"type":"error","message":"API key invalid"}
{"type":"models","models":[{"id":"anthropic/claude-sonnet-4","name":"Claude Sonnet 4"},...]}
```

**Protocol messages explained:**

| Message | Direction | Purpose |
|---------|-----------|---------|
| `user_message` | → sidecar | User's chat input |
| `tool_result` | → sidecar | Rust's response to a tool call from sidecar |
| `set_config` | → sidecar | Update provider/key/model configuration |
| `history` | → sidecar | Hydrate conversation history on startup (sent once after spawn) |
| `shutdown` | → sidecar | Graceful exit signal (sidecar exits its stdin read loop) |
| `chunk` | ← sidecar | Streaming text token from LLM |
| `tool_call` | ← sidecar | Sidecar requests data from Rust backend |
| `done` | ← sidecar | Response complete, includes usage/cost metadata |
| `error` | ← sidecar | Error message (invalid key, rate limit, etc.) |
| `models` | ← sidecar | Response to `set_config` — available models for validation |

**Not in v1 (deferred):**
- `cancel` message for interrupting in-progress streaming — user must wait for `done`

### Sidecar Lifecycle

- Spawned lazily on first assistant interaction (not on app startup)
- Stays alive for the app session (no cold start per request)
- On spawn, Rust sends a `history` message with recent messages loaded from SQLite
- On app close, Rust sends `shutdown` via stdin, then kills the child process (separate from PTY `killAll()` which runs in the frontend)

## Tools (v1)

The assistant has three tools, implemented as pi-agent-core `AgentTool` definitions. Tool execution is relayed back to the Rust backend via the stdin/stdout protocol. Rust performs the actual git/SQLite operations and returns results.

### `get_all_sessions`

- **Input:** none
- **Purpose:** Global awareness of all running/completed work
- **Output schema:**

```json
[
  {
    "id": 1,
    "status": "Running",
    "agent": "claude",
    "branch": "feature-auth",
    "repo_name": "myapp",
    "repo_path": "/home/user/myapp",
    "worktree_path": "/home/user/racc-worktrees/myapp/feature-auth",
    "elapsed_minutes": 12,
    "created_at": "2026-03-13T10:30:00Z"
  }
]
```

**Implementation notes:**
- `repo_name` and `repo_path` resolved by joining `sessions` → `repos` table via `repo_id`
- `elapsed_minutes` computed as `(now - created_at)` in the Rust handler, not stored
- `worktree_path` may be `null` if session runs directly in the repo (no worktree)

### `get_session_diff`

- **Input:** `session_id: number`
- **Purpose:** Read any session's changes on demand
- **Output:** Raw `git diff HEAD` string

**Implementation — session ID to path resolution:**

The Rust handler must resolve `session_id` to a filesystem path:

1. Query SQLite: `SELECT worktree_path, repo_id FROM sessions WHERE id = ?`
2. If `worktree_path` is not null → use it as the git diff working directory
3. If `worktree_path` is null → query `SELECT path FROM repos WHERE id = ?` using `repo_id` and use the repo path
4. Run `git diff HEAD` in the resolved directory
5. Return the raw diff string (or empty string if no changes)

This reuses the logic of the existing `get_diff` command in `git.rs` but adds the session-to-path resolution layer.

### `get_session_costs`

- **Input:** `session_id: number`
- **Purpose:** Cost context for any session
- **Output:** Project-level token counts and estimated cost USD

**Implementation notes and limitations:**

The existing `get_project_costs` command reads Claude Code JSONL files from `~/.claude/projects/{encoded_path}/*.jsonl`. These files are keyed by project path, not by Racc session ID.

**v1 limitation:** Cost data is per-project-path, not per-session. If multiple sessions share the same worktree path or repo path, their costs will be aggregated. The tool returns project-level costs for the path associated with the given session, with a note that per-session granularity is not available.

Resolution logic: same session-to-path resolution as `get_session_diff`, then call the existing `get_project_costs` logic with that path.

### Data Flow Example

```
User: "what did my agents change?"
  → LLM calls get_all_sessions()
  ← [{id:1, status:"Running", branch:"feature-auth", repo_name:"myapp", elapsed_minutes:12}, ...]
  → LLM calls get_session_diff(1)
  ← "diff --git a/auth/middleware.ts b/auth/middleware.ts\n..."
  → LLM synthesizes summary with risk categories
  ← streams markdown response to user
```

## System Prompt

```
You are the Racc assistant — a global operations butler for a developer
running multiple AI coding agents in parallel.

Today's date: {current_date}

Your primary job: help the developer understand and review what their
agents have done, without requiring them to read every line of every diff.

When summarizing changes:
- Lead with a high-level summary (what changed, why it likely changed)
- Categorize files by review priority:
  HIGH: security-sensitive, architectural, config, database
  MEDIUM: business logic, API changes
  LOW: tests, types, formatting, generated files
- Flag specific concerns (unparameterized SQL, hardcoded secrets,
  missing error handling, breaking API changes)
- Be concise — the developer has multiple agents to review

You have access to all sessions, their diffs, and their costs.
Answer questions about any session's work.
```

Note: This prompt is intentionally narrow for v1. It will be broadened as capabilities expand. `{current_date}` is injected at runtime.

## Conversation History

### Storage

All messages persisted in SQLite — one global conversation, no per-session scoping.

```sql
CREATE TABLE assistant_messages (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  role TEXT NOT NULL,        -- 'user' | 'assistant' | 'tool_call' | 'tool_result'
  content TEXT NOT NULL,     -- message text or JSON for structured content
  tool_name TEXT,            -- for tool_call and tool_result roles
  tool_call_id TEXT,         -- links tool_call to its tool_result
  created_at TEXT NOT NULL DEFAULT (datetime('now'))
);
```

**Role definitions:**
- `user` — user's chat input (content is plain text)
- `assistant` — LLM text response (content is markdown text)
- `tool_call` — LLM requesting a tool (content is JSON: `{"name":"...","args":{...}}`)
- `tool_result` — tool execution result (content is the tool's output text/JSON)

This four-role schema preserves the full tool call → result pairing needed for conversation hydration. On restart, the LLM sees the complete chain: assistant decided to call a tool → tool returned data → assistant synthesized a response.

### Context Strategy

- **MVP:** Store everything in SQLite. Send system prompt + last N messages to LLM (simple truncation to fit context window).
- **Future:** Summarize older history into memory files. Load recent conversation + relevant memories. pi-agent-core's `transformContext` hook is the integration point for smart pruning.

### Hydration

On sidecar startup, Rust loads recent messages from SQLite and sends them via the `history` protocol message. The sidecar populates its `AgentState.messages` from this payload. The assistant conversation persists across app restarts.

## Provider Configuration

### v1: OpenRouter Only

OpenRouter is the sole supported provider for v1. It provides access to all major models (Anthropic, OpenAI, Google, etc.) through a single API key.

### Setup UI (shown when no API key configured)

```
┌──────────────────────────────┐
│  Assistant Setup             │
│                              │
│  Provider: [OpenRouter    ▾] │  ← dropdown (only OpenRouter for v1)
│  API Key:  [sk-or-...     ] │  ← input field
│  Model:    [Loading...    ▾] │  ← populated by fetching OpenRouter /models API
│            [anthropic/clau ▾] │  ← user selects; fetch validates the key
│                              │
│  [Save]                      │
└──────────────────────────────┘
```

- After the user enters an API key, fetch available models from OpenRouter API
- This serves dual purpose: validates the key AND lets users pick their model
- Configuration stored in SQLite
- Future: add more providers to the dropdown (Anthropic direct, OpenAI direct, Google, etc.)

### Config Storage

```sql
CREATE TABLE assistant_config (
  key TEXT PRIMARY KEY,
  value TEXT NOT NULL
);
-- Keys: 'provider', 'api_key', 'model'
```

**API key security:** v1 stores the API key as plaintext in SQLite at `~/.racc/racc.db`. This is acceptable for a local-only desktop app MVP. Future versions should migrate to OS keychain integration (macOS Keychain, Windows Credential Manager, Linux Secret Service) via `tauri-plugin-stronghold` or similar.

## UI Design

### Location

Replaces the ActivityLog placeholder in the right panel. Layout becomes:

```
┌──────────────────────────────┐
│  Cost            ▾ $12.34   │  ← CostTracker (existing, stays compact)
├──────────────────────────────┤
│  Assistant        $0.03 ⓘ   │  ← header with assistant's own LLM cost
│──────────────────────────────│
│                              │
│  [message list, scrollable]  │
│                              │
│  Assistant and user messages │
│  rendered as markdown with   │
│  code blocks, headings,      │
│  lists, etc.                 │
│                              │
├──────────────────────────────┤
│  [Summarize Diff] [Costs]    │  ← quick action buttons (pre-fill prompts)
│  [Ask about your agents... ] │  ← text input
│                        [Send]│
└──────────────────────────────┘
```

### Components

| Component | Purpose |
|-----------|---------|
| `AssistantPanel.tsx` | Main container — replaces ActivityLog in App.tsx |
| `AssistantSetup.tsx` | Provider/key/model configuration (shown when unconfigured) |
| `AssistantChat.tsx` | Message list + input field |
| `AssistantMessage.tsx` | Single message bubble with markdown rendering (`react-markdown`) |

### State Management

A new Zustand store `assistantStore.ts` manages:
- `messages: AssistantMessage[]` — conversation messages for rendering
- `isStreaming: boolean` — whether the assistant is currently generating
- `streamingText: string` — partial text during streaming
- `config: { provider, apiKey, model } | null` — current configuration
- `assistantCost: number` — cumulative assistant LLM cost in USD
- Actions: `sendMessage()`, `appendChunk()`, `setConfig()`, `loadHistory()`

### Behavior

- **Streaming:** Assistant responses stream in token-by-token via sidecar chunks
- **Markdown:** Responses rendered with `react-markdown` (headings, code blocks, lists, bold)
- **Quick actions:** Contextual buttons above input that pre-fill common prompts ("Summarize current diff")
- **Empty state:** When no API key configured, shows AssistantSetup
- **Assistant cost:** Small label in header showing the assistant's own LLM spend, updated from `done` message usage data

## What Gets Built

### New Files

| File | Purpose |
|------|---------|
| `sidecar/` | TypeScript project — pi-ai, pi-agent-core, tools, system prompt |
| `sidecar/src/index.ts` | Sidecar entry point — stdin/stdout JSON protocol |
| `sidecar/src/tools.ts` | Tool definitions (get_all_sessions, get_session_diff, get_session_costs) |
| `sidecar/src/config.ts` | Provider detection and model configuration |
| `sidecar/package.json` | Dependencies: pi-ai, pi-agent-core |
| `sidecar/build.sh` | Cross-platform compilation script |
| `src-tauri/src/commands/assistant.rs` | Rust commands — sidecar spawn, message relay, SQLite persistence |
| `src/components/Assistant/AssistantPanel.tsx` | Main assistant panel |
| `src/components/Assistant/AssistantSetup.tsx` | Provider/key/model config UI |
| `src/components/Assistant/AssistantChat.tsx` | Chat message list + input |
| `src/components/Assistant/AssistantMessage.tsx` | Single message with markdown rendering |
| `src/stores/assistantStore.ts` | Zustand store for assistant state |

### Modified Files

| File | Change |
|------|--------|
| `src/App.tsx` | Replace `<ActivityLog />` with `<AssistantPanel />` |
| `src-tauri/src/lib.rs` | Register new assistant commands |
| `src-tauri/src/commands/mod.rs` | Add assistant module |
| `src-tauri/src/commands/db.rs` | Add assistant_messages and assistant_config tables to schema migration |
| `src-tauri/tauri.conf.json` | Add `externalBin` for sidecar binary |
| `src-tauri/capabilities/default.json` | Fix description from "OTTE" to "Racc" |
| `package.json` | Add `react-markdown` dependency |

### Removed

| File | Reason |
|------|--------|
| `src/components/ActivityLog/ActivityLog.tsx` | Replaced by AssistantPanel |

## What Does NOT Get Built (Deferred)

- Standalone settings page (v1 uses inline setup in the panel)
- Smart context management (memory files, summarization, pruning)
- DiffViewer.tsx enhancement (assistant renders analysis as markdown)
- Additional providers beyond OpenRouter
- Capabilities #2–4 (session narration, cross-session queries, review guidance)
- Agent output streaming into assistant context (real-time agent awareness)
- Streaming cancellation (`cancel` protocol message)
- OS keychain integration for API key storage
- Per-session cost granularity (blocked by Claude Code JSONL structure)
