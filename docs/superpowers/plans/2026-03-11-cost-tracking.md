# Basic Cost Tracking Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Display estimated costs per Racc session by parsing Claude Code's local JSONL session files.

**Architecture:** Rust backend parses `~/.claude/projects/<encoded-path>/*.jsonl` files, extracts `message.usage` token counts, applies model-specific pricing, and returns structured cost data via Tauri IPC. React frontend polls this data every 10s and displays it in the CostTracker panel.

**Tech Stack:** Rust (serde, serde_json), Tauri 2.x IPC, React 19, TypeScript, Zustand

**Spec:** `docs/superpowers/specs/2026-03-11-cost-tracking-design.md`
**Issue:** [#5](https://github.com/liu1700/otte/issues/5)
**Note:** No test framework is configured yet. Verification uses `cargo check`, `bun run build`, and manual testing via `bun tauri dev`.

---

## File Map

| Action | File | Responsibility |
|--------|------|---------------|
| Rewrite | `src-tauri/src/commands/cost.rs` | Parse JSONL files, calculate costs, expose Tauri command |
| Modify | `src-tauri/src/lib.rs:18` | Update command registration (`get_usage` → `get_project_costs`) |
| Create | `src/types/cost.ts` | TypeScript types matching Rust cost structs |
| Rewrite | `src/components/CostTracker/CostTracker.tsx` | Wire to backend, poll, display cost breakdown |
| Modify | `src/components/Sidebar/NewSessionDialog.tsx:9-14` | Simplify AGENTS array to Claude Code only |

---

## Chunk 1: Backend — Rust Cost Module

### Task 1: Rewrite `cost.rs` with JSONL parsing and model-specific pricing

**Files:**
- Rewrite: `src-tauri/src/commands/cost.rs`

- [ ] **Step 1: Replace the entire `cost.rs` with the new implementation**

Replace the full contents of `src-tauri/src/commands/cost.rs` with:

```rust
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::PathBuf;

// --- Pricing per 1M tokens ---

struct ModelPricing {
    input: f64,
    output: f64,
    cache_write: f64,
    cache_read: f64,
}

const OPUS_PRICING: ModelPricing = ModelPricing {
    input: 15.0,
    output: 75.0,
    cache_write: 18.75,
    cache_read: 1.50,
};

const SONNET_PRICING: ModelPricing = ModelPricing {
    input: 3.0,
    output: 15.0,
    cache_write: 3.75,
    cache_read: 0.30,
};

const HAIKU_PRICING: ModelPricing = ModelPricing {
    input: 0.80,
    output: 4.0,
    cache_write: 1.0,
    cache_read: 0.08,
};

fn pricing_for_model(model: &str) -> &'static ModelPricing {
    let lower = model.to_lowercase();
    if lower.contains("opus") {
        &OPUS_PRICING
    } else if lower.contains("haiku") {
        &HAIKU_PRICING
    } else {
        // Default to Sonnet pricing (covers "sonnet" and unknown models)
        &SONNET_PRICING
    }
}

// --- JSONL deserialization types ---

#[derive(Deserialize)]
struct JsonlLine {
    message: Option<MessagePayload>,
}

#[derive(Deserialize)]
struct MessagePayload {
    model: Option<String>,
    usage: Option<UsageFields>,
}

#[derive(Deserialize)]
struct UsageFields {
    input_tokens: Option<u64>,
    output_tokens: Option<u64>,
    cache_creation_input_tokens: Option<u64>,
    cache_read_input_tokens: Option<u64>,
}

// --- Return types ---

#[derive(Debug, Clone, Serialize)]
pub struct SessionCost {
    pub session_id: String,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_creation_tokens: u64,
    pub cache_read_tokens: u64,
    pub estimated_cost_usd: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProjectCosts {
    pub sessions: Vec<SessionCost>,
    pub total_input_tokens: u64,
    pub total_output_tokens: u64,
    pub total_cache_creation_tokens: u64,
    pub total_cache_read_tokens: u64,
    pub total_estimated_cost_usd: f64,
}

// --- Core logic ---

fn parse_jsonl_file(path: &std::path::Path) -> SessionCost {
    let session_id = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("unknown")
        .to_string();

    let mut input_tokens: u64 = 0;
    let mut output_tokens: u64 = 0;
    let mut cache_creation_tokens: u64 = 0;
    let mut cache_read_tokens: u64 = 0;
    let mut estimated_cost: f64 = 0.0;

    let file = match fs::File::open(path) {
        Ok(f) => f,
        Err(_) => {
            return SessionCost {
                session_id,
                input_tokens: 0,
                output_tokens: 0,
                cache_creation_tokens: 0,
                cache_read_tokens: 0,
                estimated_cost_usd: 0.0,
            };
        }
    };

    let reader = BufReader::new(file);
    for line in reader.lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => continue, // skip unreadable lines
        };

        if line.is_empty() {
            continue;
        }

        let parsed: JsonlLine = match serde_json::from_str(&line) {
            Ok(p) => p,
            Err(_) => continue, // skip malformed lines
        };

        let message = match parsed.message {
            Some(m) => m,
            None => continue,
        };

        let usage = match message.usage {
            Some(u) => u,
            None => continue,
        };

        let model_name = message.model.as_deref().unwrap_or("sonnet");
        let pricing = pricing_for_model(model_name);

        let inp = usage.input_tokens.unwrap_or(0);
        let out = usage.output_tokens.unwrap_or(0);
        let cw = usage.cache_creation_input_tokens.unwrap_or(0);
        let cr = usage.cache_read_input_tokens.unwrap_or(0);

        input_tokens += inp;
        output_tokens += out;
        cache_creation_tokens += cw;
        cache_read_tokens += cr;

        estimated_cost += (inp as f64 * pricing.input / 1_000_000.0)
            + (out as f64 * pricing.output / 1_000_000.0)
            + (cw as f64 * pricing.cache_write / 1_000_000.0)
            + (cr as f64 * pricing.cache_read / 1_000_000.0);
    }

    SessionCost {
        session_id,
        input_tokens,
        output_tokens,
        cache_creation_tokens,
        cache_read_tokens,
        estimated_cost_usd: (estimated_cost * 100.0).round() / 100.0,
    }
}

fn encode_path(path: &str) -> String {
    path.replace('/', "-")
}

fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}

// --- Tauri command ---

#[tauri::command]
pub async fn get_project_costs(worktree_path: String) -> Result<ProjectCosts, String> {
    let home = home_dir().ok_or("Could not find home directory")?;
    let encoded = encode_path(&worktree_path);
    let project_dir = home.join(".claude").join("projects").join(&encoded);

    if !project_dir.exists() {
        return Ok(ProjectCosts {
            sessions: vec![],
            total_input_tokens: 0,
            total_output_tokens: 0,
            total_cache_creation_tokens: 0,
            total_cache_read_tokens: 0,
            total_estimated_cost_usd: 0.0,
        });
    }

    let mut sessions: Vec<SessionCost> = Vec::new();

    if let Ok(entries) = fs::read_dir(&project_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("jsonl") {
                sessions.push(parse_jsonl_file(&path));
            }
        }
    }

    let total_input: u64 = sessions.iter().map(|s| s.input_tokens).sum();
    let total_output: u64 = sessions.iter().map(|s| s.output_tokens).sum();
    let total_cache_creation: u64 = sessions.iter().map(|s| s.cache_creation_tokens).sum();
    let total_cache_read: u64 = sessions.iter().map(|s| s.cache_read_tokens).sum();
    let total_cost: f64 = sessions.iter().map(|s| s.estimated_cost_usd).sum();

    Ok(ProjectCosts {
        sessions,
        total_input_tokens: total_input,
        total_output_tokens: total_output,
        total_cache_creation_tokens: total_cache_creation,
        total_cache_read_tokens: total_cache_read,
        total_estimated_cost_usd: (total_cost * 100.0).round() / 100.0,
    })
}
```

- [ ] **Step 2: Update command registration in `lib.rs`**

In `src-tauri/src/lib.rs`, change `commands::cost::get_usage` to `commands::cost::get_project_costs`.

- [ ] **Step 3: Verify Rust compiles**

Run: `cd src-tauri && cargo check`
Expected: no errors

- [ ] **Step 4: Commit backend changes**

```bash
git add src-tauri/src/commands/cost.rs src-tauri/src/lib.rs
git commit -m "feat(cost): rewrite cost module to parse Claude Code JSONL session files (#5)"
```

---

## Chunk 2: Frontend — Types, CostTracker, Agent Dropdown

### Task 2: Create TypeScript cost types

**Files:**
- Create: `src/types/cost.ts`

- [ ] **Step 1: Create `src/types/cost.ts`**

```typescript
export interface SessionCost {
  session_id: string;
  input_tokens: number;
  output_tokens: number;
  cache_creation_tokens: number;
  cache_read_tokens: number;
  estimated_cost_usd: number;
}

export interface ProjectCosts {
  sessions: SessionCost[];
  total_input_tokens: number;
  total_output_tokens: number;
  total_cache_creation_tokens: number;
  total_cache_read_tokens: number;
  total_estimated_cost_usd: number;
}
```

- [ ] **Step 2: Commit**

```bash
git add src/types/cost.ts
git commit -m "feat(cost): add TypeScript types for cost tracking (#5)"
```

---

### Task 3: Wire CostTracker component to backend

**Files:**
- Rewrite: `src/components/CostTracker/CostTracker.tsx`

- [ ] **Step 1: Replace CostTracker with wired implementation**

Replace the full contents of `src/components/CostTracker/CostTracker.tsx` with:

```tsx
import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useSessionStore } from "../../stores/sessionStore";
import type { ProjectCosts } from "../../types/cost";

const COST_POLL_INTERVAL_MS = 10_000;

function formatTokens(n: number): string {
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`;
  if (n >= 1_000) return `${(n / 1_000).toFixed(1)}k`;
  return n.toString();
}

export function CostTracker() {
  const [costs, setCosts] = useState<ProjectCosts | null>(null);
  const activeSessionId = useSessionStore((s) => s.activeSessionId);
  const sessions = useSessionStore((s) => s.sessions);

  const activeSession = sessions.find((s) => s.id === activeSessionId);
  const worktreePath = activeSession?.worktree_path;

  useEffect(() => {
    if (!worktreePath) {
      setCosts(null);
      return;
    }

    let cancelled = false;

    const fetchCosts = async () => {
      try {
        const data = await invoke<ProjectCosts>("get_project_costs", {
          worktreePath,
        });
        if (!cancelled) setCosts(data);
      } catch {
        // Silent fail — cost tracking is non-critical
      }
    };

    fetchCosts();
    const interval = setInterval(fetchCosts, COST_POLL_INTERVAL_MS);
    return () => {
      cancelled = true;
      clearInterval(interval);
    };
  }, [worktreePath]);

  return (
    <div className="border-b border-surface-3 bg-surface-1 px-4 py-3">
      <h2 className="text-xs font-semibold uppercase tracking-wider text-zinc-400">
        Cost
      </h2>
      <div className="mt-2 grid grid-cols-2 gap-3">
        <div>
          <p className="text-xs text-zinc-500">Total cost</p>
          <p className="text-lg font-semibold text-white">
            ${costs?.total_estimated_cost_usd.toFixed(2) ?? "0.00"}
          </p>
        </div>
        <div>
          <p className="text-xs text-zinc-500">Sessions</p>
          <p className="text-lg font-semibold text-white">
            {costs?.sessions.length ?? 0}
          </p>
        </div>
        <div>
          <p className="text-xs text-zinc-500">Input tokens</p>
          <p className="text-sm text-zinc-300">
            {formatTokens(costs?.total_input_tokens ?? 0)}
          </p>
        </div>
        <div>
          <p className="text-xs text-zinc-500">Output tokens</p>
          <p className="text-sm text-zinc-300">
            {formatTokens(costs?.total_output_tokens ?? 0)}
          </p>
        </div>
        <div>
          <p className="text-xs text-zinc-500">Cache write</p>
          <p className="text-sm text-zinc-300">
            {formatTokens(costs?.total_cache_creation_tokens ?? 0)}
          </p>
        </div>
        <div>
          <p className="text-xs text-zinc-500">Cache read</p>
          <p className="text-sm text-zinc-300">
            {formatTokens(costs?.total_cache_read_tokens ?? 0)}
          </p>
        </div>
      </div>
    </div>
  );
}
```

- [ ] **Step 2: Verify frontend builds**

Run: `bun run build`
Expected: no errors

- [ ] **Step 3: Commit**

```bash
git add src/components/CostTracker/CostTracker.tsx
git commit -m "feat(cost): wire CostTracker to backend with polling (#5)"
```

---

### Task 4: Simplify agent dropdown to Claude Code only

**Files:**
- Modify: `src/components/Sidebar/NewSessionDialog.tsx:9-14`

- [ ] **Step 1: Replace the AGENTS array**

In `src/components/Sidebar/NewSessionDialog.tsx`, replace lines 9-14:

```typescript
// Old:
const AGENTS = [
  { id: "claude-code", label: "Claude Code" },
  { id: "aider", label: "Aider" },
  { id: "codex", label: "Codex" },
  { id: "shell", label: "Shell (bash)" },
];

// New:
const AGENTS = [
  { id: "claude-code", label: "Claude Code" },
];
```

- [ ] **Step 2: Verify frontend builds**

Run: `bun run build`
Expected: no errors

- [ ] **Step 3: Commit**

```bash
git add src/components/Sidebar/NewSessionDialog.tsx
git commit -m "feat(ui): simplify agent dropdown to Claude Code only (#5)"
```

---

## Chunk 3: Integration Verification

### Task 5: Full build and manual test

- [ ] **Step 1: Run full Rust build**

Run: `cd src-tauri && cargo build`
Expected: compiles without errors

- [ ] **Step 2: Run full frontend build**

Run: `bun run build`
Expected: no errors

- [ ] **Step 3: Manual test with `bun tauri dev`**

Run: `bun tauri dev`

Verify:
1. App launches without errors
2. Agent dropdown only shows "Claude Code"
3. CostTracker panel shows cost data (if you have Claude Code sessions in `~/.claude/projects/`)
4. If no sessions exist, CostTracker shows $0.00 and 0 tokens gracefully

- [ ] **Step 4: Update roadmap**

In `wiki/Roadmap.md`, change the Basic cost tracking row status from `**Next**` to `Done (#5)`.

- [ ] **Step 5: Commit roadmap update**

```bash
git add wiki/Roadmap.md
git commit -m "docs: mark basic cost tracking as done in roadmap (#5)"
```
