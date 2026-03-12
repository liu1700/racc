# Basic Cost Tracking — Design Spec

**Issue:** [#5](https://github.com/liu1700/otte/issues/5)
**Date:** 2026-03-11
**Status:** Approved

## Goal

Display estimated cost per OTTE session and aggregate totals by parsing Claude Code's local session JSONL files. Also simplify the agent dropdown to Claude Code only (MVP scope).

## Data Source

Claude Code stores API usage in per-session JSONL files:

- **Location:** `~/.claude/projects/<encoded-project-path>/<session-uuid>.jsonl`
- **Encoded path format:** absolute path with `/` replaced by `-` (e.g., `-Users-yuchenliu-Documents-otte`)
- **Usage data:** embedded in assistant message objects as `message.usage`:
  ```json
  {
    "message": {
      "model": "claude-opus-4-6",
      "usage": {
        "input_tokens": 3,
        "cache_creation_input_tokens": 11296,
        "cache_read_input_tokens": 9105,
        "output_tokens": 9
      }
    }
  }
  ```
- Each JSONL line is a JSON object; only lines with `message.usage` are relevant.

## Architecture

### Data Flow

```
~/.claude/projects/<encoded-path>/<session-uuid>.jsonl
  → Rust cost.rs: parse JSONL, extract message.usage + message.model
  → Aggregate tokens per file, apply model-specific pricing
  → Tauri command returns UsageData to frontend
  → CostTracker.tsx polls via invoke(), displays breakdown
```

### Backend — `src-tauri/src/commands/cost.rs`

**Rewrite** the existing `cost.rs` module. Remove the current `get_usage()` that reads from non-existent `~/.claude/usage/`.

**New types:**

```rust
struct SessionCost {
    session_id: String,        // JSONL filename (UUID)
    input_tokens: u64,
    output_tokens: u64,
    cache_creation_tokens: u64,
    cache_read_tokens: u64,
    estimated_cost_usd: f64,
}

struct ProjectCosts {
    sessions: Vec<SessionCost>,
    total_input_tokens: u64,
    total_output_tokens: u64,
    total_cache_creation_tokens: u64,
    total_cache_read_tokens: u64,
    total_estimated_cost_usd: f64,
}
```

**New commands:**

1. `get_project_costs(worktree_path: String) -> Result<ProjectCosts, String>`
   - Convert `worktree_path` to encoded path format
   - Scan `~/.claude/projects/<encoded-path>/` for `*.jsonl` files
   - For each JSONL file: read line-by-line, parse JSON, extract `message.usage` and `message.model`
   - Aggregate tokens, apply model-specific pricing
   - Return `ProjectCosts` with per-session breakdown

**Pricing constants (per 1M tokens):**

| Model Pattern | Input | Output | Cache Write | Cache Read |
|---------------|-------|--------|-------------|------------|
| `opus` | $15.00 | $75.00 | $18.75 | $1.50 |
| `sonnet` | $3.00 | $15.00 | $3.75 | $0.30 |
| `haiku` | $0.80 | $4.00 | $1.00 | $0.08 |

Model matching: check if `message.model` string contains "opus", "sonnet", or "haiku". Default to Sonnet pricing if unknown.

**Performance:** JSONL files can grow large. For MVP, parse the full file each time. If this becomes a bottleneck, we can add byte-offset caching later.

### Frontend — `src/components/CostTracker/CostTracker.tsx`

**Wire up the existing placeholder component:**

1. Define TypeScript types matching Rust structs:
   ```typescript
   interface SessionCost {
     session_id: string;
     input_tokens: number;
     output_tokens: number;
     cache_creation_tokens: number;
     cache_read_tokens: number;
     estimated_cost_usd: number;
   }

   interface ProjectCosts {
     sessions: SessionCost[];
     total_input_tokens: number;
     total_output_tokens: number;
     total_cache_creation_tokens: number;
     total_cache_read_tokens: number;
     total_estimated_cost_usd: number;
   }
   ```

2. `useEffect` with `setInterval` (10s) to call `invoke("get_project_costs", { worktreePath })`.
3. Get `worktreePath` from the active session in the Zustand store.
4. Display:
   - Total estimated cost (prominent, large text)
   - Token breakdown: input / output / cache
   - Per-session cost list (if multiple Claude Code sessions exist for the project)

### Agent Dropdown — `src/components/Sidebar/NewSessionDialog.tsx`

Simplify the `AGENTS` array to only contain Claude Code:

```typescript
const AGENTS = [
  { id: "claude-code", label: "Claude Code" },
];
```

Keep the `<select>` element and `agent` state so re-adding agents later is trivial.

## What's NOT in Scope

- Persistent cost history / database
- Cost alerts or budgets
- User-configurable pricing
- Aider / Codex cost tracking (v0.2 multi-agent)
- tmux-based cost capture
- Cost data in status bar (can be added later)

## Success Criteria

- CostTracker shows non-zero costs when Claude Code sessions have usage data
- Token breakdown is accurate (input, output, cache creation, cache read)
- Costs refresh automatically every 10 seconds
- Gracefully handles missing `~/.claude/projects/` or empty JSONL files
- Agent dropdown only shows Claude Code
