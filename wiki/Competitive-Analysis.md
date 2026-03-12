# Competitive Analysis

[< Home](Home.md) | [< Session Lifecycle](Session-Lifecycle.md)

## Landscape

Racc occupies a unique position: it is **not** a coding AI, **not** a code editor, but an **orchestration layer** for existing coding agents.

## Comparison Matrix

| Dimension | Racc | CLI Tools (Claude Squad, Codeman) | IDE Agents (Cursor, Windsurf) |
|-----------|------|-----------------------------------|-------------------------------|
| **Multi-session** | Native visual dashboard | CLI or simple WebUI | Single-session sidebar |
| **Agent choice** | Any terminal agent | Usually tied to one agent | Locked to vendor model |
| **Cost tracking** | Built-in, first-class | Requires third-party tools | None or minimal |
| **Env isolation** | Worktree + Docker Sandbox | Worktree only | No explicit isolation |
| **Remote support** | Tailscale native | Manual SSH | Partial |
| **Change review** | PR-level diff view | None | Inline diff only |

## Primary Competitive Moat

### 1. Pluggable Agents (strongest differentiator)

Cursor locks users into their models. Windsurf locks into Codeium. Racc lets users **choose which agent to use per task**.

Why this matters now: AI model capabilities are evolving rapidly. A new model drops every few weeks. Users want to try Claude Code today, switch to Aider tomorrow, test Codex next week. Vendor lock-in is the #1 risk in the agentic coding space.

Racc's agent-agnostic design means users never have to choose between their IDE and the best available agent.

### 2. Session Resilience (secondary moat)

Native PTY management with SQLite persistence provides:
- Graceful cleanup on app close
- Session reconciliation on restart (orphaned sessions marked Disconnected)
- Output buffer replay when switching between sessions

Planned for v0.2: Tailscale + remote execution for true fire-and-forget workflows (start at work, agent keeps running on VPS, review at home).

## What We Do NOT Compete On

- **Code editing** — we don't have an editor, and that's by design
- **AI model quality** — we use whatever agents the user chooses
- **IDE features** — no autocomplete, no language servers, no debugging

This is a deliberate non-compete strategy. We complement existing tools rather than replacing them.

## Threat Assessment

| Threat | Likelihood | Mitigation |
|--------|------------|------------|
| Claude Code adds built-in multi-session UI | Medium | Stay agent-agnostic; support agents Anthropic won't |
| Cursor/Windsurf add agent-agnostic mode | Low | Their business model depends on vendor lock-in |
| Open-source CLI tools become "good enough" | Medium | Visual UX and cross-machine management are hard to replicate in CLI |
| New entrant builds same product | Medium | Ship fast, build community, iterate on UX |

[Next: Roadmap >](Roadmap.md)
