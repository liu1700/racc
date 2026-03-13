# Racc Assistant — Global AI Butler for Multi-Agent Management

**Date:** 2026-03-13
**Status:** Approved
**Scope:** v1 MVP — diff summary & risk triage

## Problem

When coding agents change dozens or hundreds of files, developers don't review them. A traditional diff viewer doesn't solve this — the volume itself is the problem. Developers need an intelligent layer that triages changes, highlights risks, and lets them ask follow-up questions.

## Solution

A global AI assistant ("butler") that lives in Racc's right panel, powered by pi-ai and pi-agent-core running as a Tauri sidecar. It maintains one continuous conversation, sees all sessions, and can analyze any session's diffs on demand.

v1 capability: diff summary & risk triage only. Future capabilities (session narration, cross-session queries, review guidance) will be added incrementally.

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

### Message Protocol (JSON lines over stdin/stdout)

**Frontend → Rust → Sidecar:**

```json
{"type":"user_message","content":"what did my agents change?"}
{"type":"tool_result","call_id":"x","content":"diff --git a/..."}
{"type":"set_config","provider":"openrouter","api_key":"sk-or-...","model":"anthropic/claude-sonnet-4"}
```

**Sidecar → Rust → Frontend:**

```json
{"type":"chunk","text":"## Summary\n..."}
{"type":"tool_call","id":"x","name":"get_session_diff","args":{"session_id":1}}
{"type":"done"}
{"type":"error","message":"API key invalid"}
{"type":"models","models":[{"id":"anthropic/claude-sonnet-4","name":"Claude Sonnet 4"},...]}
```

### Sidecar Lifecycle

- Spawned lazily on first assistant interaction (not on app startup)
- Stays alive for the app session (no cold start per request)
- Killed on app close alongside PTY processes (existing killAll pattern)

## Tools (v1)

The assistant has three tools, implemented as pi-agent-core `AgentTool` definitions. Tool execution is relayed back to the Rust backend, which performs the actual git/SQLite operations.

| Tool | Input | Output | Purpose |
|------|-------|--------|---------|
| `get_all_sessions` | none | Session list with status, agent, branch, elapsed time, repo path | Global awareness |
| `get_session_diff` | `session_id: number` | Raw `git diff HEAD` for that session's worktree | Read any session's changes |
| `get_session_costs` | `session_id: number` | Token counts + estimated cost USD | Cost context |

### Data Flow Example

```
User: "what did my agents change?"
  → LLM calls get_all_sessions()
  ← [{id:1, status:"Running", branch:"feature-auth", repo:"myapp", elapsed:"12m"}, ...]
  → LLM calls get_session_diff(1)
  ← "diff --git a/auth/middleware.ts b/auth/middleware.ts\n..."
  → LLM synthesizes summary with risk categories
  ← streams markdown response to user
```

## System Prompt

```
You are the Racc assistant — a global operations butler for a developer
running multiple AI coding agents in parallel.

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

Note: This prompt is intentionally narrow for v1. It will be broadened as capabilities expand.

## Conversation History

### Storage

All messages persisted in SQLite — one global conversation, no per-session scoping.

```sql
CREATE TABLE assistant_messages (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  role TEXT NOT NULL,        -- 'user' | 'assistant' | 'tool_result'
  content TEXT NOT NULL,     -- message text or JSON for tool calls/results
  tool_name TEXT,            -- null for non-tool messages
  tool_call_id TEXT,         -- null for non-tool messages
  created_at TEXT NOT NULL DEFAULT (datetime('now'))
);
```

### Context Strategy

- **MVP:** Store everything in SQLite. Send system prompt + last N messages to LLM (simple truncation to fit context window).
- **Future:** Summarize older history into memory files. Load recent conversation + relevant memories. pi-agent-core's `transformContext` hook is the integration point for smart pruning.

### Hydration

On app startup, load recent messages from SQLite into sidecar's `AgentState`. The assistant conversation persists across restarts.

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
- Configuration stored in SQLite (new `assistant_config` table or similar)
- Future: add more providers to the dropdown (Anthropic direct, OpenAI direct, Google, etc.)

### Config Storage

```sql
CREATE TABLE assistant_config (
  key TEXT PRIMARY KEY,
  value TEXT NOT NULL
);
-- Keys: 'provider', 'api_key', 'model'
```

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
| `AssistantMessage.tsx` | Single message bubble with markdown rendering |

### Behavior

- **Streaming:** Assistant responses stream in token-by-token via sidecar chunks
- **Markdown:** Responses rendered with full markdown support (headings, code blocks, lists, bold)
- **Quick actions:** Contextual buttons above input that pre-fill common prompts ("Summarize current diff")
- **Empty state:** When no API key configured, shows AssistantSetup
- **Assistant cost:** Small label in header showing the assistant's own LLM spend (separate from agent costs)

## What Gets Built

### New Files

| File | Purpose |
|------|---------|
| `sidecar/` | TypeScript project — pi-ai, pi-agent-core, tools, system prompt |
| `sidecar/src/index.ts` | Sidecar entry point — stdin/stdout JSON protocol |
| `sidecar/src/tools.ts` | Tool definitions (get_all_sessions, get_session_diff, get_session_costs) |
| `sidecar/src/config.ts` | Provider detection and model configuration |
| `src-tauri/src/commands/assistant.rs` | Rust commands — sidecar spawn, message relay, SQLite persistence |
| `src/components/Assistant/AssistantPanel.tsx` | Main assistant panel |
| `src/components/Assistant/AssistantSetup.tsx` | Provider/key/model config UI |
| `src/components/Assistant/AssistantChat.tsx` | Chat message list + input |
| `src/components/Assistant/AssistantMessage.tsx` | Single message with markdown rendering |

### Modified Files

| File | Change |
|------|--------|
| `src/App.tsx` | Replace `<ActivityLog />` with `<AssistantPanel />` |
| `src-tauri/src/lib.rs` | Register new assistant commands |
| `src-tauri/src/commands/mod.rs` | Add assistant module |
| `src-tauri/src/db.rs` | Add assistant_messages and assistant_config tables |

### Removed

| File | Reason |
|------|--------|
| `src/components/ActivityLog/ActivityLog.tsx` | Replaced by AssistantPanel |

## What Does NOT Get Built (Deferred)

- In-app settings page (v1 uses the inline setup in the panel)
- Smart context management (memory files, summarization, pruning)
- DiffViewer.tsx enhancement (assistant renders analysis as markdown)
- Additional providers beyond OpenRouter
- Capabilities #2–4 (session narration, cross-session queries, review guidance)
- Agent output streaming into assistant context (future — would let assistant see what agents are doing in real-time)
