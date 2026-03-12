# Competitive Analysis

[< Home](Home.md) | [< Session Lifecycle](Session-Lifecycle.md)

## Landscape

OTTE occupies a unique position: it is **not** a coding AI, **not** a code editor, but an **orchestration layer** for existing coding agents.

## Comparison Matrix

| Dimension | OTTE | CLI Tools (Claude Squad, Codeman) | IDE Agents (Cursor, Windsurf) |
|-----------|------|-----------------------------------|-------------------------------|
| **Multi-session** | Native visual dashboard | CLI or simple WebUI | Single-session sidebar |
| **Agent choice** | Any terminal agent | Usually tied to one agent | Locked to vendor model |
| **Cost tracking** | Built-in, first-class | Requires third-party tools | None or minimal |
| **Env isolation** | Worktree + Docker Sandbox | Worktree only | No explicit isolation |
| **Remote support** | Tailscale native | Manual SSH | Partial |
| **Change review** | PR-level diff view | None | Inline diff only |

## Primary Competitive Moat

### 1. Pluggable Agents (strongest differentiator)

Cursor locks users into their models. Windsurf locks into Codeium. OTTE lets users **choose which agent to use per task**.

Why this matters now: AI model capabilities are evolving rapidly. A new model drops every few weeks. Users want to try Claude Code today, switch to Aider tomorrow, test Codex next week. Vendor lock-in is the #1 risk in the agentic coding space.

OTTE's agent-agnostic design means users never have to choose between their IDE and the best available agent.

### 2. Session Immortality (secondary moat)

tmux + Tailscale enables true fire-and-forget workflows:
- Start a task at work
- Close laptop
- Agent keeps working on VPS
- Review results at home

No competing product offers this level of session resilience for agentic coding.

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
