# TODOS

Items deferred from the Agent Supervisor Pattern eng review (2026-03-21).

## Deferred

### Shell prompt detection for spawn reliability
- **What:** Replace the 100ms `sleep()` in `create_session()` with PTY output watching for a shell prompt before sending the agent command.
- **Why:** The sleep is arbitrary — a slow machine may not have the shell ready in 100ms. Currently acceptable because the supervisor spawns one session per 5s tick.
- **Context:** Eng review issue #5. Accepted as-is for now. Revisit if spawn failures occur under load.
- **Depends on:** Health monitor's PTY output subscription (same infrastructure).

### Webhook / desktop notification integration
- **What:** Add optional webhook delivery and native desktop notifications (Tauri notification API) for supervisor events.
- **Why:** Users who walk away need to know when tasks complete or fail without checking the dashboard.
- **Context:** Design doc lists as "optional." In-app notifications (toast/badge) should be validated first.
- **Depends on:** Supervisor notification system (Phase 3).

### Task dependency graphs
- **What:** Allow tasks to declare dependencies on other tasks. Supervisor respects ordering.
- **Why:** Some workflows have natural sequencing (e.g., "fix the API, then update the tests").
- **Context:** Design doc open question #1. Simple priority queue sufficient for initial implementation.
- **Depends on:** Task scheduler in supervisor.rs.

### Cost / token tracking per agent session
- **What:** Track API token usage and estimated cost per session and per task.
- **Why:** Running 10 agents in parallel can get expensive. Users need visibility into spend.
- **Context:** Not in scope for supervisor MVP. Requires agent-specific API integrations.
- **Depends on:** Agent adapters in agent.rs (need to parse cost output from each agent type).

### Setup simplification
- **What:** One-command install script, auto-configuration, first-run wizard for remote servers.
- **Why:** Setup friction is a barrier to adoption (design doc gap #1).
- **Context:** Extracted from supervisor design as a separate concern. Should be its own design doc.
- **Depends on:** Nothing — independent workstream.
