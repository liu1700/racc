# UI Design

[< Home](Home) | [< Feature Specification](Feature-Specification)

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

This is a deliberate departure from Cursor/Windsurf, which squeeze agents into side panels. OTTE's users are migrating from full-screen terminal agent workflows. The terminal must remain the primary interaction surface.

## Left Sidebar — Session List

- Vertical list of all sessions (active, paused, disconnected, completed)
- Each entry shows: session name, status indicator (color dot), agent type icon
- Quick action buttons: New Session, Pause, Terminate
- Drag to reorder
- Right-click context menu for advanced operations

## Center Main Area — Dual Mode

Switches between two modes:

### Terminal Mode (default)
- Full xterm.js terminal rendering the active agent session
- Supports all ANSI escape sequences, colors, cursor movement
- Input goes directly to the agent via PTY/send-keys
- Terminal toolbar: font size, copy mode, search within output

### Diff Review Mode
- Activated when agent completes a task or on user request
- Side-by-side diff view (left = before, right = after)
- File tree navigator on the left edge
- Per-file and per-hunk accept/reject buttons
- Checkpoint selector to compare against any historical state

## Right Panel — Intelligence Dashboard

Three collapsible sections:

### Activity Log
- Chronological timeline of agent operations
- Icons per event type (file read, command exec, search, decision)
- Click to expand details
- Filter buttons: All | Files | Commands | Decisions

### Cost Dashboard
- Current session cost (tokens + dollars)
- Mini chart showing cost over time
- Alert indicator when approaching threshold

### File Change List
- Files modified in current session
- Status badges: Added (green), Modified (yellow), Deleted (red)
- Click to jump to diff view for that file

## Global Status Bar

Fixed bottom bar showing:
- Total accumulated cost
- Current week's spend
- Subscription quota usage (progress bar)
- Number of active sessions
- Network status (connected/disconnected per machine)

[Next: Technical Architecture >](Technical-Architecture)
