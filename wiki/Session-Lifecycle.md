# Session Lifecycle

[< Home](Home) | [< Technical Architecture](Technical-Architecture)

## Session Creation Flow

When a user clicks "New Session", they specify: **target machine**, **project**, **branch**, and **agent type**. Everything else is automated:

```
User clicks "New Session"
        |
        v
[1] Environment Preparation
    - Create git worktree on target machine
    - (Optional) Spin up Docker Sandbox
        |
        v
[2] Session Persistence
    - Create named tmux session: otte-{project}-{branch}
    - Set working directory to worktree path
        |
        v
[3] Agent Startup
    - Launch specified coding agent inside tmux session
        |
        v
[4] Network Configuration
    - Register Portless naming
    - (If remote) Configure Tailscale Serve
        |
        v
[5] Communication Channel
    - Establish IDE <-> tmux PTY/send-keys channel
        |
        v
[6] State Registration
    - Register session in dashboard
    - Begin monitoring (cost, activity, status)
```

## State Machine

```
                    +----------+
  User clicks  ---->| Creating |
  "New Session"     +----+-----+
                         |
                    success / failure
                    /              \
               +---v---+      +---v---+
               |Running|      | Error |
               +---+---+      +-------+
                   |
          +--------+--------+
          |        |        |
     agent asks  user     network
     for input   pauses   drops
          |        |        |
     +----v---+ +--v---+ +-v-----------+
     |Waiting | |Paused| |Disconnected |
     +----+---+ +--+---+ +------+------+
          |        |             |
     user responds | user       IDE reconnects
          |        | resumes    |
          +---+----+      +-----+
              |            |
          +---v---+   +---v---+
          |Running|   |Running|
          +---+---+   +-------+
              |
         agent exits
              |
         +----v-----+
         |Completed  |
         +----------+
```

### State Definitions

| State | Meaning | Entry Trigger | User Can... |
|-------|---------|---------------|-------------|
| **Creating** | Setting up worktree, sandbox, tmux | User clicks "New Session" | Wait or cancel |
| **Running** | Agent is actively executing | Creation completes / resume / input provided | View terminal, pause, stop |
| **Waiting** | Agent needs user input or approval | Agent asks a question / permission request | Respond, approve/reject |
| **Paused** | User-initiated pause | User clicks "Pause" | Resume, stop, review changes |
| **Disconnected** | IDE lost connection, agent still running | SSH/network interruption | Reconnect (auto or manual) |
| **Completed** | Agent finished its task | Agent process exits cleanly | Review changes, start new task |
| **Error** | Agent crashed or exited abnormally | Process crash / non-zero exit | View logs, retry, stop |

### Key Design: "Session Immortality"

The **Disconnected** state is the cornerstone of OTTE's session model.

**Scenario:** Developer closes laptop at the office. Agent continues working on the remote VPS inside tmux. Developer opens laptop at home. IDE auto-discovers the tmux session, reattaches, and the dashboard updates — as if nothing happened.

This is possible because:
1. The agent runs inside tmux, not inside the IDE
2. tmux sessions persist across any client disconnection
3. The daemon on each machine continuously tracks session state
4. The IDE client is stateless — it queries the daemon on connect

### Session Cleanup

When a session reaches **Completed** or is terminated:

1. Prompt user: keep or delete the worktree?
2. If delete: remove git worktree, destroy tmux session, tear down Docker container (if any)
3. Archive session metadata (cost, activity log, duration) for historical tracking
4. Remove from active dashboard, add to session history

[Next: Competitive Analysis >](Competitive-Analysis)
