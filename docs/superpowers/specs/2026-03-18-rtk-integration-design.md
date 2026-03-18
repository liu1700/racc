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
- **Racc-isolated** — Binary lives at `$HOME/.racc/bin/rtk`, hook at `$HOME/.racc/hooks/rtk-rewrite.sh`. Does not install to system paths.

## Design

### 1. RTK Binary Management

**Location:** `$HOME/.racc/bin/rtk` (resolved via `std::env::var("HOME")`, never tilde)

A new `rtk.rs` module in `racc-core/src/` manages the binary lifecycle.

**Download logic:**
- On first Claude Code session creation, check if `$HOME/.racc/bin/rtk` exists and is executable.
- If missing, detect platform and download from a pinned release version.
- Download to a temp file first (`$HOME/.racc/bin/.rtk.tmp`), then `std::fs::rename()` into place (atomic on POSIX) to prevent corruption from concurrent session creation.
- `chmod +x` the binary before the atomic rename.

**Pinned version:** Use a const in the code (e.g., `const RTK_VERSION: &str = "0.5.0"`). The download URL pattern:
```
https://github.com/rtk-ai/rtk/releases/download/v{VERSION}/rtk-{TARGET}.tar.gz
```

**Platform mapping table:**

| `std::env::consts` (OS, ARCH) | GitHub Release asset name |
|-------------------------------|--------------------------|
| `("linux", "x86_64")` | `rtk-x86_64-unknown-linux-musl` |
| `("linux", "aarch64")` | `rtk-aarch64-unknown-linux-gnu` |
| `("macos", "x86_64")` | `rtk-x86_64-apple-darwin` |
| `("macos", "aarch64")` | `rtk-aarch64-apple-darwin` |

Remote platform mapping (from `uname` output):

| `uname -s` | `uname -m` | GitHub Release asset name |
|-------------|------------|--------------------------|
| `Linux` | `x86_64` | `rtk-x86_64-unknown-linux-musl` |
| `Linux` | `aarch64` | `rtk-aarch64-unknown-linux-gnu` |
| `Darwin` | `x86_64` | `rtk-x86_64-apple-darwin` |
| `Darwin` | `arm64` | `rtk-aarch64-apple-darwin` |

Unsupported platform combinations log a warning and skip rtk setup.

**Dependency:** `reqwest = { version = "0.12", default-features = false, features = ["rustls-tls"] }` — avoids pulling in OpenSSL; aligns with the existing `russh` TLS stack.

**Error handling:** If download fails (no internet, rate limited, unsupported platform), log a warning and proceed. Session creation is never blocked.

### 2. Claude Code Hook Configuration

**Hook script:** `$HOME/.racc/hooks/rtk-rewrite.sh`

**Preferred approach:** Use rtk's own hook script. Running `rtk init --hook-only` generates `~/.claude/hooks/rtk-rewrite.sh` and patches `~/.claude/settings.json`. During implementation, check if `rtk init --hook-only` (or `rtk init -g --hook-only`) can be run with `RTK_BIN` pointing to `$HOME/.racc/bin/rtk`. If so, just run it — rtk already handles the hook script content and settings merge correctly. Racc only needs to ensure the binary is on PATH.

**Fallback approach:** If rtk's init doesn't support custom paths, Racc generates a minimal hook script that delegates JSON I/O to rtk itself (no python3/jq dependency):

```bash
#!/bin/bash
# Racc-managed rtk rewrite hook for Claude Code PreToolUse
RTK_BIN="$HOME/.racc/bin/rtk"
if [ ! -x "$RTK_BIN" ]; then
  exit 0  # rtk not available, pass through
fi
# Delegate to rtk's built-in hook handler which reads stdin and writes stdout
exec "$RTK_BIN" hook --pre-tool-use
```

If rtk lacks a `hook` subcommand, fall back to a script that uses `$RTK_BIN --rewrite` with proper input sanitization. The exact implementation will be determined during development based on rtk's actual CLI interface. The hook script must not depend on `python3` or `jq` — only on `rtk` itself and POSIX shell builtins.

**Claude Code settings merge algorithm:**

1. Read `$HOME/.claude/settings.json`. If it doesn't exist, create it with `{"hooks": {"PreToolUse": [<entry>]}}` and return.
2. Parse as JSON. If parse fails, log warning and skip (don't corrupt user's file).
3. Ensure `hooks` key exists and is an object. If missing, create it.
4. Ensure `hooks.PreToolUse` key exists and is an array. If missing, create it.
5. Check if any element in the array already has `"hook"` matching the rtk hook path. If found, skip.
6. Append `{"matcher": "Bash", "hook": "<absolute-path>/.racc/hooks/rtk-rewrite.sh"}` to the array, where `<absolute-path>` is the resolved home directory (e.g., `/home/user`), not `$HOME` or `~`.
7. Write back to `$HOME/.claude/settings.json` with pretty-print formatting.

Create parent directories (`$HOME/.claude/`) if they don't exist.

**Idempotency:** No flag file. Instead, check for the actual artifacts on each session creation:
- Binary: `Path::new(rtk_bin_path).exists()` — if missing, download.
- Hook script: `Path::new(hook_path).exists()` — if missing, write it.
- Settings: read and check for hook entry — if missing, merge it.

These are simple file existence checks (essentially free) and avoid the stale-flag problem.

### 3. Session Integration

#### Local sessions

In `create_session()`, before spawning the local Claude Code PTY:

1. Call `ensure_rtk_local()` — idempotent binary download + hook configuration.
2. If rtk binary is available, build env map with `PATH=$HOME/.racc/bin:$PATH`.
3. Pass env map to `LocalPtyTransport::spawn()`.

**`LocalPtyTransport::spawn()` change:** Accept an optional `extra_env: Option<HashMap<String, String>>` parameter. Apply each entry via `cmd_builder.env(key, value)` in addition to the existing `TERM=xterm-256color`. Default is `None` for non-Claude-Code callers.

**`reattach_session()` change:** Also calls `ensure_rtk_local()` and passes the env map to `LocalPtyTransport::spawn()` when reattaching Claude Code sessions. Same logic as `create_session()`.

#### Remote sessions (SSH)

In `create_session()` with `server_id`, before spawning `SshTmuxTransport`:

1. Call `ensure_rtk_remote(ssh_manager, server_id)`:
   - Check: `ssh exec "test -x $HOME/.racc/bin/rtk && echo ok || echo missing"`
   - If missing, detect remote platform: `ssh exec "uname -s && uname -m"`
   - Download directly on remote: `ssh exec "mkdir -p $HOME/.racc/bin && curl -fsSL -o $HOME/.racc/bin/.rtk.tmp <release_url> && chmod +x $HOME/.racc/bin/.rtk.tmp && mv $HOME/.racc/bin/.rtk.tmp $HOME/.racc/bin/rtk"`
   - Write hook script on remote via base64 transport: `ssh exec "mkdir -p $HOME/.racc/hooks && echo '<base64-encoded-script>' | base64 --decode > $HOME/.racc/hooks/rtk-rewrite.sh && chmod +x $HOME/.racc/hooks/rtk-rewrite.sh"`
   - Merge remote settings.json: `ssh exec "cat $HOME/.claude/settings.json 2>/dev/null || echo '{}'"` to read, modify in Rust using the same merge algorithm as local, then write back via base64: `ssh exec "echo '<base64-encoded-json>' | base64 --decode > $HOME/.claude/settings.json"`. If the file doesn't exist, create it with parent dirs. This is the same pattern as the hook script transport.
2. PATH injection for tmux: Bake PATH into the agent command string itself since `SshTmuxTransport::spawn()` has no env parameter:
   ```
   PATH=$HOME/.racc/bin:$PATH claude --dangerously-skip-permissions 'task'
   ```
   This is done in `build_agent_command()` when rtk is available on the remote, not by modifying `SshTmuxTransport`. Note: `$HOME` and `$PATH` expand correctly here because tmux executes the session command via `sh -c`, which performs variable expansion even though the outer SSH command single-quotes the tmux argument.

**Agent gating:** Only run rtk setup when `agent == "claude-code"`.

### 4. Module Structure

**New files:**

- `racc-core/src/rtk.rs` — All rtk management logic:
  - `ensure_rtk_local() -> bool` — Local binary + hook setup, returns whether rtk is available
  - `ensure_rtk_remote(ssh_manager, server_id) -> bool` — Remote setup via SSH, returns whether rtk is available
  - `download_rtk_binary(target_dir: &Path) -> Result<()>` — HTTP download with atomic rename
  - `configure_claude_hook(home_dir: &Path)` — Write hook script + merge settings.json
  - `configure_claude_hook_remote(ssh_manager, sid)` — Remote equivalent via base64 transport
  - `platform_asset_name() -> Option<String>` — Local platform detection
  - `remote_platform_asset_name(uname_output: &str) -> Option<String>` — Remote platform detection

**Modified files:**

- `racc-core/Cargo.toml` — Add `reqwest = { version = "0.12", default-features = false, features = ["rustls-tls"] }`
- `racc-core/src/lib.rs` — Add `pub mod rtk;`
- `racc-core/src/commands/session.rs`:
  - `create_session()` — Call `ensure_rtk_local()` or `ensure_rtk_remote()` for Claude Code sessions
  - `reattach_session()` — Call `ensure_rtk_local()`/`ensure_rtk_remote()` for Claude Code sessions; pass `extra_env` to local `spawn()` and `rtk_available` to `build_agent_command()` for remote reattach
  - `build_agent_command()` — Accept optional `rtk_available` flag; if true on remote, prepend `PATH=$HOME/.racc/bin:$PATH` to command
- `racc-core/src/transport/local_pty.rs` — Add `extra_env: Option<HashMap<String, String>>` to `spawn()`

**No changes to:**

- Frontend (`src/`)
- Tauri app (`src-tauri/src/`)
- Server (`racc-server/`)
- DB schema
- `SshTmuxTransport` — remote PATH is handled via agent command string

## Data Flow

```
create_session(agent="claude-code")
  ├─ local ──→ ensure_rtk_local()
  │              ├─ $HOME/.racc/bin/rtk exists? → skip download
  │              ├─ missing → download to .rtk.tmp, atomic rename
  │              ├─ hook script exists? → skip
  │              └─ settings.json has hook? → skip merge
  │            LocalPtyTransport::spawn(extra_env: {PATH: $HOME/.racc/bin:$PATH})
  │
  └─ remote ─→ ensure_rtk_remote(ssh, sid)
                 ├─ ssh: test -x rtk → skip download
                 ├─ missing → ssh: curl + atomic rename
                 ├─ ssh: hook script via base64
                 └─ ssh: merge remote settings.json
               build_agent_command(rtk_available=true)
                 → "PATH=$HOME/.racc/bin:$PATH claude ..."

reattach_session(agent="claude-code")
  ├─ local ──→ ensure_rtk_local() → same as above
  └─ remote ─→ ensure_rtk_remote() → same as above
```

## What This Does NOT Include

- Token savings analytics or UI
- Aider/Codex support
- rtk auto-updates (version is pinned in code; bump manually)
- Custom TOML filter management
- Any frontend changes
- Windows support
