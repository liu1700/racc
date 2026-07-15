# UI Design

[< Home](Home.md) | [< Feature Specification](Feature-Specification.md) | [Cognitive Design Research](Cognitive-Design-Research.md)

This page documents the current interface. The research rationale behind attention, chunking, and progressive disclosure remains in [Cognitive Design Research](Cognitive-Design-Research.md).

## Layout

```text
+----------------------+-----------------------------------------------------+
| Repository/session   | Tasks | Terminal | Servers                          |
| sidebar              |                                                     |
|                      | Four-column task board, terminal, or server panel   |
+----------------------+-----------------------------------------------------+
| Session status counts | token usage | connection status                  |
+----------------------------------------------------------------------------+
```

The terminal is a primary workspace, not a narrow chat sidebar. It remains mounted when another tab is selected so xterm state and selection are not recreated on every switch.

## Repository and Session Sidebar

The left sidebar is repository-first. Each imported repository expands to show its sessions.

Each session row can show:

- status dot and running pulse;
- branch name;
- elapsed time;
- latest non-empty terminal output while running;
- a detected GitHub pull-request link;
- reattach/reconnect, stop, and remove actions appropriate to its state.

Clicking a session calls the backend reconnect path before selecting it. This is intentionally idempotent: an already connected session stays attached, a live remote tmux transport is restored, and a disconnected local session follows the full reattach flow.

Repositories expose actions for starting work, importing/removing repositories, and creating normal sessions. Sessions remain visible after completion until explicitly removed.

### Session Status Colors

| Status | Color role | Meaning |
|--------|------------|---------|
| Running | Green | Transport is active |
| Completed | Blue | Session ended normally or remote tmux is gone |
| Waiting/Paused | Amber | Agent needs attention |
| Disconnected | Orange | Metadata exists but transport is unavailable |
| Error | Red | Creation or operation failed |

Color is reinforced by text and available actions; users do not need to infer critical state from color alone.

## Tasks Tab

The board is a four-column CSS grid. Equal columns and constrained text prevent live output from resizing the layout.

### Open

- Inline multiline task creation and editing.
- Clipboard-pasted or file-selected images with thumbnail preview.
- **Generate tasks with AI** opens Task Planner.
- **Fire** opens agent, permission, location, server, worktree, and branch options.

Task Planner accepts an Epic link or description, runs Claude Code or Codex read-only, and shows a dependency-aware preview. No task is selected by default. Selecting a dependent item automatically selects its prerequisites; removing a prerequisite removes selected dependents. Only confirmed items enter Open.

### Working

Cards show the associated session, branch, elapsed time, latest activity, attachments, and PR state. A task with a valid GitHub pull-request URL can be marked **Ready to merge** and added to Merge Manager.

When the associated session completes or is removed, the task is assigned the archived `closed` status and disappears from the active board. The Tasks badge counts non-archived tasks.

### Merge Manager

Merge Manager contains:

- ordered, removable queued PR cards;
- per-repository target branch, agent, and ship instructions;
- an active-run card linking to the manager terminal;
- last-run status and summary;
- `needs_review` actions: **Mark succeeded**, **Mark failed**, and **Retry**;
- a **Reset** action that clears the queue and run history while preserving settings;
- the **Ship All** action with queued count.

Succeeded items leave the visible queue. Failed or ambiguous items remain available for review/retry.

### Test Manager

Test Manager mirrors the manager interaction pattern without a PR queue:

- per-repository target branch, agent, and test instructions;
- default comprehensive full-project UAT instructions that the user can overwrite;
- active-run terminal link;
- last-run status, passed/failed counts, evidence summary, and recovery actions;
- a **Reset** action that clears run history while preserving settings;
- a bottom action labelled **Run**.

The column describes the run as isolated and read-only. Test Manager does not present commit, push, or merge controls.

## Terminal Tab

- xterm.js 5.5 with responsive `FitAddon` sizing.
- Input flows to the selected backend transport; output is streamed and retained in a bounded backend buffer.
- Buffer replay restores recent output when switching sessions or reconnecting a client.
- IME-sensitive shifted punctuation is handled at the keyboard boundary.
- A placeholder is shown when no session is selected.

### Links

Racc registers separate terminal link behaviors:

- HTTP(S) links open directly in the system browser through Tauri's shell plugin or `window.open` in browser mode. Racc only accepts the `http:` and `https:` schemes.
- Detected repository paths open the built-in file viewer, including an optional line number.

This avoids xterm's generic dangerous-link confirmation for ordinary browser links while keeping scheme validation in Racc.

## File Viewer and Command Palette

The file viewer is an overlay, so it does not consume permanent layout space.

| Shortcut | Action |
|----------|--------|
| `Cmd+P` | Open fuzzy repository file search |
| `Cmd+F` | Search within the open file |
| `Ctrl+G` | Jump to a line |
| `Enter` / `Shift+Enter` | Next/previous search match |
| `Esc` | Close the innermost active layer |

Search respects `.gitignore`; the viewer uses Shiki syntax highlighting and reports truncation when a backend line limit is applied.

## Servers Tab

The Servers panel manages SSH targets. Users can import SSH config entries or add a host manually, test the connection, connect/disconnect, edit/remove it, and run remote setup. A server can then be selected when launching a task or normal agent session.

## Status Bar

The bottom bar summarizes non-zero session counts by state, total/weekly Claude Code token usage, and frontend connection state. This is ambient context, not a substitute for manager result evidence.

## Desktop and Browser Differences

The same React components run in both modes. Native file pickers, shell opening, and desktop notifications use dynamically loaded Tauri plugins on desktop. Browser mode uses WebSocket for commands/events/terminal data and standard browser APIs where safe.

The headless browser surface is operationally equivalent for core workflows, but its deployment currently relies on a trusted network because `racc-server` has no application-level auth.

## Inactive or Future UI

The repository contains preserved insight/supervisor experiments that are not part of the current visible product. Future designs should not be described as current UI until they are reachable and their backend behavior is enabled.

[Next: Technical Architecture >](Technical-Architecture.md)
