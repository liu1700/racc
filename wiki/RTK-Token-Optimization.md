# RTK Token Optimization

[< Home](Home.md) | [< Technical Architecture](Technical-Architecture.md)

## Overview

Racc integrates [rtk](https://github.com/rtk-ai/rtk) (Rust Token Killer) to automatically reduce Claude Code token consumption by 60-90%. This is transparent to the agent — Racc handles installation and configuration; Claude Code sessions get optimized output with zero manual setup.

## How It Works

rtk is a CLI proxy that sits between Claude Code and shell commands. When Claude Code runs `git status`, rtk intercepts the call via a `PreToolUse` hook and rewrites it to `rtk git status`. rtk then executes the real command, filters the output (stripping ANSI codes, verbose test output, boilerplate noise), and returns only the semantically important information to the agent's context window.

```
Claude Code Bash tool call
       |
       v
PreToolUse hook (rtk-rewrite.sh)
       |
       v
rtk rewrite "git status" → "rtk git status"
       |
       v
rtk executes real command, filters output
       |
       v
Compressed output returned to Claude Code (60-90% fewer tokens)
```

## Integration Architecture

All rtk logic lives in `racc-core/src/rtk.rs`. Session creation triggers setup automatically.

### Local Sessions

```
create_session(agent="claude-code")
  → ensure_rtk_local()
     ├─ Binary exists at ~/.racc/bin/rtk? → skip
     ├─ Missing → download from GitHub Releases (atomic: .tmp → rename)
     └─ Configure hook (rtk init -g --hook-only --auto-patch)
  → LocalPtyTransport::spawn(extra_env: {PATH: ~/.racc/bin:$PATH})
```

### Remote Sessions (SSH)

```
create_session(agent="claude-code", server_id=sid)
  → ensure_rtk_remote(ssh_manager, sid)
     ├─ ssh: test -x ~/.racc/bin/rtk → skip
     ├─ Missing → ssh: curl + tar + atomic rename
     └─ Configure hook via rtk init or base64-encoded script
  → build_agent_command(rtk_remote=true)
     → "PATH=$HOME/.racc/bin:$PATH claude ..."
```

### Key Properties

- **Non-blocking**: Session creation never fails because of rtk. All errors log warnings and proceed without optimization.
- **Claude Code only**: Aider and Codex sessions are unaffected.
- **Racc-isolated**: Binary at `~/.racc/bin/rtk`, not installed to system paths.
- **Idempotent**: Each session creation checks actual artifacts (binary, hook, settings) — no stale flag files.
- **Atomic downloads**: Uses temp file + rename to prevent corruption from concurrent session creation.

## Files

| File | Purpose |
|------|---------|
| `racc-core/src/rtk.rs` | All rtk logic: download, hook config, ensure_local/remote |
| `racc-core/src/commands/session.rs` | Integration: calls ensure_rtk in create/reattach |
| `racc-core/src/transport/local_pty.rs` | `extra_env` parameter for PATH injection |
| `~/.racc/bin/rtk` | Downloaded rtk binary (pinned version) |
| `~/.claude/settings.json` | Claude Code hook configuration (managed by rtk init) |

## Dependencies Added

- `reqwest` (rustls-tls) — HTTP download of rtk binary
- `flate2` + `tar` — Tarball extraction
- `base64` — Safe SSH transport for remote hook scripts
