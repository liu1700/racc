# RTK Integration Design — Automatic Token Cost Reduction for Claude Code Sessions

**Date:** 2026-03-18
**Status:** Approved
**Scope:** Auto-install rtk and configure Claude Code hooks for local and remote sessions

## Problem

When AI coding agents execute shell commands, the raw output (ANSI codes, verbose test output, boilerplate warnings, formatting noise) consumes tokens unnecessarily. A 30-minute Claude Code session can waste tens of thousands of tokens on output the LLM doesn't need.

## Solution

Racc transparently installs [rtk](https://github.com/rtk-ai/rtk) (Rust Token Killer) and configures Claude Code's `PreToolUse` hook so that shell commands are automatically rewritten to their rtk equivalents. This reduces token consumption by 60-90% with zero agent awareness.

## Constraints

- **Claude Code only** — Aider and Codex are not affected.
- **No UI** — No new frontend components, settings, or analytics surfaces.
- **Non-blocking** — Session creation must never fail because of rtk. If download or setup fails, log a warning and proceed without it.
- **Racc-isolated** — Binary lives at `~/.racc/bin/rtk`, hook at `~/.racc/hooks/rtk-rewrite.sh`. Does not install to system paths.

## Design

### 1. RTK Binary Management

**Location:** `~/.racc/bin/rtk`

A new `rtk.rs` module in `racc-core/src/` manages the binary lifecycle.

**Download logic:**
- On first Claude Code session creation, check if `~/.racc/bin/rtk` exists.
- If missing, detect platform at runtime via `(std::env::consts::OS, std::env::consts::ARCH)` mapping to GitHub Release asset names (e.g., `rtk-x86_64-unknown-linux-gnu`, `rtk-aarch64-apple-darwin`).
- Download via HTTP using `reqwest` (new dependency for `racc-core`).
- `chmod +x` the binary.
- No version tracking — just check file existence.

**Error handling:** If download fails (no internet, rate limited, etc.), log a warning and proceed. Session creation is never blocked.

### 2. Claude Code Hook Configuration

**Hook script:** `~/.racc/hooks/rtk-rewrite.sh`

A shell script that:
- Receives tool input JSON from Claude Code on stdin.
- Extracts the `command` field.
- Rewrites eligible commands by prepending `~/.racc/bin/rtk`.
- Returns modified JSON to stdout.

**Claude Code settings merge:** Racc reads existing `~/.claude/settings.json`, parses as JSON, checks if the rtk hook entry already exists in `hooks.PreToolUse`. If not, appends:

```json
{
  "hooks": {
    "PreToolUse": [
      {
        "matcher": "Bash",
        "hook": "~/.racc/hooks/rtk-rewrite.sh"
      }
    ]
  }
}
```

Never overwrites other hooks or settings the user has configured.

**Idempotency:** A flag file (`~/.racc/.rtk-configured`) marks that setup is complete. Subsequent session creations skip the check entirely — just file existence.

### 3. Session Integration

#### Local sessions

In `create_session()`, before spawning the local Claude Code PTY:

1. Call `ensure_rtk_local()` — idempotent binary download + hook configuration.
2. If rtk is available, build env map with `PATH=~/.racc/bin:$PATH`.
3. Pass env map to `LocalPtyTransport::spawn()`.

**`LocalPtyTransport::spawn()` change:** Accept an optional `env: HashMap<String, String>` parameter. Apply each entry via `cmd_builder.env(key, value)`. Currently only sets `TERM=xterm-256color`; this extends that pattern.

#### Remote sessions (SSH)

In `create_session()` with `server_id`, before spawning `SshTmuxTransport`:

1. Call `ensure_rtk_remote(ssh_manager, server_id)`:
   - Check: `ssh exec "test -x ~/.racc/bin/rtk && echo ok || echo missing"`
   - If missing, detect remote platform: `ssh exec "uname -s && uname -m"`
   - Download directly on remote: `ssh exec "mkdir -p ~/.racc/bin && curl -fsSL -o ~/.racc/bin/rtk <release_url> && chmod +x ~/.racc/bin/rtk"`
   - Configure hook: write hook script and merge `~/.claude/settings.json` on remote via SSH exec commands.
2. Spawn tmux with PATH prepended: `PATH=~/.racc/bin:$PATH tmux new-session -d -s racc-{id} '{agent_cmd}'`

**Agent gating:** Only run rtk setup when `agent == "claude-code"`.

### 4. Module Structure

**New files:**

- `racc-core/src/rtk.rs` — All rtk management logic:
  - `ensure_rtk_local()` — Local binary + hook setup
  - `ensure_rtk_remote(ssh_manager, server_id)` — Remote setup via SSH
  - `download_rtk_binary(target_dir, platform, arch)` — HTTP download helper
  - `configure_claude_hook(base_path)` — Write hook script + merge settings.json
  - `detect_platform()` / `detect_remote_platform(ssh_manager, sid)` — Platform detection

**Modified files:**

- `racc-core/Cargo.toml` — Add `reqwest` dependency
- `racc-core/src/lib.rs` — Add `pub mod rtk;`
- `racc-core/src/commands/session.rs` — Call `ensure_rtk_local()` or `ensure_rtk_remote()` in `create_session()`
- `racc-core/src/transport/local_pty.rs` — Accept optional env vars in `spawn()`

**No changes to:**

- Frontend (`src/`)
- Tauri app (`src-tauri/src/`)
- Server (`racc-server/`)
- DB schema

## Data Flow

```
create_session(agent="claude-code")
  ├─ local ──→ ensure_rtk_local()
  │              ├─ ~/.racc/bin/rtk exists? → skip
  │              └─ missing → download from GitHub Releases
  │            configure hook (if not already done)
  │            LocalPtyTransport::spawn(env: {PATH: ~/.racc/bin:$PATH})
  │
  └─ remote ─→ ensure_rtk_remote(ssh, sid)
                 ├─ ssh: test -x ~/.racc/bin/rtk → skip
                 └─ missing → ssh: curl download
               configure remote hook
               PATH=~/.racc/bin:$PATH tmux new-session ...
```

## What This Does NOT Include

- Token savings analytics or UI
- Aider/Codex support
- rtk version management or auto-updates
- Custom TOML filter management
- Any frontend changes
