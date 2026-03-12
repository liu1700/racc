# UI Design

[< Home](Home.md) | [< Feature Specification](Feature-Specification.md)

## Layout Overview

Three-panel layout, left to right:

```
+----------------+------------------------------------+----------------------+
|                |                                    |                      |
|  Left Sidebar  |         Center Main Area           |   Right Panel        |
|  (~15%)        |         (~55%)                     |   (~30%)             |
|                |                                    |                      |
|  Session List  |  Agent Terminal (PTY / xterm.js)   |  Activity Log        |
|  + Quick       |  ── or ──                          |  Cost Dashboard      |
|    Actions     |  Diff Review View                  |  File Change List    |
|                |  (switchable)                      |                      |
|  [New]         |                                    |                      |
|  [Pause]       |                                    |                      |
|  [Stop]        |                                    |                      |
|                |                                    |                      |
+----------------+------------------------------------+----------------------+
|                        Global Status Bar                                   |
|  Total Cost: $X.XX | This Week: $X.XX | Quota: XX% | Active Sessions: N   |
+----------------------------------------------------------------------------+
```

## Critical Design Decision

**The agent terminal occupies the center main area** — not a sidebar.

This is a deliberate departure from Cursor/Windsurf, which squeeze agents into side panels. Racc's users are migrating from full-screen terminal agent workflows. The terminal must remain the primary interaction surface.

## Left Sidebar — Session List (implemented)

- Expandable repo list with nested sessions underneath each repo
- Each repo shows: name, path, expand/collapse toggle
- Each session shows: agent type, branch name, status color dot
- Status colors: running (green `#22c55e`), completed (blue `#3b82f6`), disconnected (orange `#f97316`), error (red `#ef4444`)
- Quick actions per repo: [+] Launch new session, [×] Remove repo
- Quick actions per session: Stop (if running), Remove (if not running)
- Import Repo button opens native folder picker

## Center Main Area — Terminal (implemented)

Currently terminal-only mode:

### Terminal Mode (default)
- Full xterm.js 5.5 terminal rendering the active agent session
- Dark theme: background `#111113`, cursor `#6366f1` (indigo accent)
- FitAddon for responsive sizing with ResizeObserver
- Input goes directly to the agent via PTY write
- Buffer replay on session switch (up to 1MB per session)
- Async dynamic import of xterm to avoid blocking initial render
- Placeholder message when no active session selected

### Diff Review Mode *(planned)*
- Placeholder component exists (`DiffViewer.tsx`)
- Backend `get_diff` command returns `git diff HEAD` output
- Full side-by-side review UI planned for P1

## Right Panel — Intelligence Dashboard

### Cost Dashboard (implemented)
- Polls `get_project_costs` every 10 seconds
- Displays: total estimated cost, session count
- Token breakdown: input, output, cache creation, cache read tokens
- Model-aware pricing: Opus ($15/$75), Sonnet ($3/$15), Haiku ($0.80/$4) per 1M tokens
- Silent failure if cost data is unavailable

### Activity Log *(placeholder)*
- Shows "Agent activity will appear here"
- Structured event parsing planned for P1

### File Change List *(planned)*
- Not yet implemented
- Will show files modified in current session with status badges

## Global Status Bar (implemented)

Fixed bottom bar showing:
- Number of active sessions
- Connection status indicator (green dot)
- Placeholder cost displays (to be connected to real-time aggregation)

[Next: Technical Architecture >](Technical-Architecture.md)
