# User Guide

[< Home](Home.md)

This guide describes the current Racc workflow in both the Tauri desktop app and the browser UI served by `racc-server`.

## 1. Add a Repository

Select **Import Repo** and choose an existing git repository. Racc stores repository and session metadata in `~/.racc/racc.db`; it does not copy or upload the repository.

The selected repository controls the Tasks board, usage summary, server/session list, and manager settings shown in the center panel.

## 2. Create or Generate Tasks

The Tasks view has four columns:

| Column | Purpose |
|--------|---------|
| **Open** | Draft and review work that has not started |
| **Working** | Monitor tasks connected to agent sessions |
| **Merge Manager** | Queue completed PRs and ship them in order |
| **Test Manager** | Run a repository-wide UAT pass in isolation |

Completed tasks are stored with a closed/archive status but are intentionally not shown as a separate column.

### Create a task manually

Use the input in Open. Task descriptions can span multiple lines and can include pasted or selected image attachments. Images are stored under the repository's `.racc/images/` directory and their absolute paths are included when the task is fired.

### Generate tasks with Task Planner

Choose **Generate tasks with AI**, paste an Epic URL or a complete product description, and select Claude Code or Codex. The planner runs read-only against the repository and returns a preview through Racc's scoped MCP tool.

Nothing is added automatically. Check the tasks you want; selecting a dependent task also selects its prerequisites. Confirm the preview to add the selected items to Open. If a source link needs credentials the agent cannot access, paste the source text instead.

## 3. Fire a Task

Select **Fire** on an Open task, then choose:

- Claude Code or Codex
- whether to skip the agent's permission prompts
- the repository itself or a dedicated git worktree
- a branch name when using a worktree
- a local machine or configured remote server

Worktrees are the normal choice for parallel changes. They isolate git state, but they are not an operating-system sandbox.

Racc creates the session and backend transport, starts the chosen agent, waits for the agent's ready prompt, and then sends the task description. The task moves to Working and the sidebar shows live terminal activity.

## 4. Monitor and Interact

Use the top tabs to switch between **Tasks**, **Terminal**, and **Servers**. The terminal remains mounted while hidden so its state is preserved across tab changes.

- Click a session in the sidebar to open or reconnect it.
- Terminal keyboard input is written directly to the session transport.
- Clicking an HTTP(S) link opens it in the system browser without xterm's generic dangerous-link confirmation.
- Detected repository file paths open in Racc's file viewer; an optional `:line` suffix jumps to that line.
- A detected GitHub pull-request URL is stored on the session and appears as a PR link in the sidebar and Working card.

Only `http://` and `https://` URLs are treated as external links.

## 5. Ship PRs with Merge Manager

When a Working task has a valid GitHub PR URL, choose **Ready to merge**. Racc adds it to the repository's ordered merge queue.

In Merge Manager:

1. Check queue order.
2. Set the target branch, agent, and ship instructions. Settings are saved per repository.
3. Select **Ship All**.
4. Open the manager terminal at any time to inspect progress.

The manager creates an integration worktree/branch, validates the combined change set, and merges eligible PRs in queue order. The agent must submit its structured result through `racc_merge_manager.submit_merge_result`; printing JSON in the terminal does not complete the run.

If a conflict, failed validation, changed PR head, or ambiguous final state occurs, the run stops or enters `needs_review`. Inspect the terminal and remote repository, then choose **Mark succeeded**, **Mark failed**, or **Retry** as appropriate.

Choose **Reset** when no merge run is active to clear the queue and previous merge results and return the manager to its initial state. Repository settings and terminal sessions are preserved.

## 6. Use Test Manager

Test Manager is configured per repository with a target branch, agent, and editable instructions. Its default instructions ask for a comprehensive full-project UAT pass: discover the build, run the broadest automated suite, exercise important user-visible flows end to end, and record reproducible evidence without changing product code or weakening tests.

Select **Run** to start. Racc creates an isolated `racc/test-*` worktree and a dedicated manager session. The run is read-only by policy: it must not commit, push, merge, or modify product code.

The agent reports each command or UAT scenario, status, evidence, and summary through `racc_test_manager.submit_test_result`. Racc updates the Test Manager card directly and displays passed/failed counts. Printed terminal JSON is ignored as a completion protocol.

Choose **Reset** when no test run is active to clear previous test results and return the manager to its initial state. Repository settings and terminal sessions are preserved.

## 7. Restart and Recovery

### Normal task sessions

- A local PTY cannot survive a Racc process restart. A previously Running local session becomes **Disconnected**.
- Reattach starts a new PTY in the same directory. Claude Code resumes the recorded conversation UUID when available and uses its legacy continuation fallback for older rows. Codex uses its resume flow.
- A remote SSH/tmux session can survive a Racc restart. Reconciliation probes tmux: a live session remains Running, a missing session becomes Completed, and an unreachable host becomes Disconnected.

### Planner and manager runs

Planner, Merge Manager, and Test Manager receive short-lived, capability-scoped MCP endpoints for one run. Restarting a manager session does not recreate the expired endpoint. Racc therefore marks an unresolved merge or test run `needs_review`; verify the external state and resolve it manually or retry in a fresh run.

## 8. Remote Servers and Browser Mode

Use the Servers tab to import an SSH host or enter connection details, test the connection, and run setup. Remote agent sessions use SSH with tmux so their terminal process can persist independently of the UI.

For browser access:

```bash
bun run build
cd src-tauri
RACC_DIST_PATH=../dist cargo run --bin racc-server
```

Open `http://localhost:9399`, or the host's private Tailscale address. `racc-server` binds to `0.0.0.0` and currently provides no application-level authentication or TLS, so do not expose it to an untrusted network.

## Troubleshooting

| Symptom | What to check |
|---------|---------------|
| Planner returns no tasks | The source link may require authentication; paste the complete text |
| Merge queue is empty | Mark a Working task with an exact GitHub PR URL as Ready to merge |
| Manager says `needs_review` | Inspect its terminal and GitHub state; resolve manually or Retry |
| Remote session is Disconnected | Test the server connection, then reopen/reconnect the session |
| Terminal browser link does not open | Confirm the target is an `http://` or `https://` URL |
| Test Manager fails immediately | Check the target branch, local build prerequisites, and conflicting local services |

[Next: Feature Specification >](Feature-Specification.md)
