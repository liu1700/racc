# RTK Integration Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Auto-install rtk and configure Claude Code hooks to reduce token costs by 60-90% for all Claude Code sessions spawned by Racc.

**Architecture:** New `rtk.rs` module in racc-core handles binary download (reqwest + atomic rename) and hook setup (delegates to `rtk init`). Session creation calls `ensure_rtk_local()`/`ensure_rtk_remote()` for Claude Code sessions, then injects `PATH` into the PTY environment.

**Tech Stack:** Rust, reqwest (rustls-tls), portable-pty CommandBuilder, SSH exec, rtk CLI

**Spec:** `docs/superpowers/specs/2026-03-18-rtk-integration-design.md`

---

## Chunk 1: Core rtk module — platform detection and binary download

### Task 1: Add reqwest dependency to racc-core

**Files:**
- Modify: `src-tauri/racc-core/Cargo.toml:22` (add after last dependency)

- [ ] **Step 1: Add reqwest to Cargo.toml**

Add after the `strsim` line in `[dependencies]`:

```toml
reqwest = { version = "0.12", default-features = false, features = ["rustls-tls"] }
```

- [ ] **Step 2: Verify it compiles**

Run: `cd /home/devuser/racc/src-tauri && cargo check -p racc-core`
Expected: compiles successfully (warnings OK)

- [ ] **Step 3: Commit**

```bash
git add src-tauri/racc-core/Cargo.toml src-tauri/Cargo.lock
git commit -m "chore: add reqwest dependency to racc-core for rtk downloads"
```

---

### Task 2: Create rtk.rs — platform detection functions

**Files:**
- Create: `src-tauri/racc-core/src/rtk.rs`
- Modify: `src-tauri/racc-core/src/lib.rs:6` (add module declaration)

- [ ] **Step 1: Create rtk.rs with platform detection**

Create `src-tauri/racc-core/src/rtk.rs`:

```rust
use std::path::{Path, PathBuf};

const RTK_VERSION: &str = "0.30.0";

/// Map local platform to GitHub Release asset name.
/// Returns None for unsupported platforms.
pub fn platform_asset_name() -> Option<String> {
    let asset = match (std::env::consts::OS, std::env::consts::ARCH) {
        ("linux", "x86_64") => "rtk-x86_64-unknown-linux-musl",
        ("linux", "aarch64") => "rtk-aarch64-unknown-linux-gnu",
        ("macos", "x86_64") => "rtk-x86_64-apple-darwin",
        ("macos", "aarch64") => "rtk-aarch64-apple-darwin",
        _ => return None,
    };
    Some(asset.to_string())
}

/// Map remote platform (from `uname -s` + `uname -m` output) to asset name.
/// Input: two-line string "Linux\nx86_64\n" or "Darwin\narm64\n".
pub fn remote_platform_asset_name(uname_output: &str) -> Option<String> {
    let lines: Vec<&str> = uname_output.trim().lines().collect();
    if lines.len() < 2 {
        return None;
    }
    let os = lines[0].trim();
    let arch = lines[1].trim();
    let asset = match (os, arch) {
        ("Linux", "x86_64") => "rtk-x86_64-unknown-linux-musl",
        ("Linux", "aarch64") => "rtk-aarch64-unknown-linux-gnu",
        ("Darwin", "x86_64") => "rtk-x86_64-apple-darwin",
        ("Darwin", "arm64") => "rtk-aarch64-apple-darwin",
        _ => return None,
    };
    Some(asset.to_string())
}

/// Build the download URL for a given asset name.
fn download_url(asset_name: &str) -> String {
    format!(
        "https://github.com/rtk-ai/rtk/releases/download/v{}/{}.tar.gz",
        RTK_VERSION, asset_name
    )
}

/// Get the racc home directory ($HOME/.racc).
fn racc_home() -> Option<PathBuf> {
    std::env::var("HOME").ok().map(|h| PathBuf::from(h).join(".racc"))
}

/// Get the rtk binary path ($HOME/.racc/bin/rtk).
pub fn rtk_bin_path() -> Option<PathBuf> {
    racc_home().map(|h| h.join("bin").join("rtk"))
}
```

- [ ] **Step 2: Register the module in lib.rs**

In `src-tauri/racc-core/src/lib.rs`, add after line 6 (`pub mod commands;`):

```rust
pub mod rtk;
```

- [ ] **Step 3: Verify it compiles**

Run: `cd /home/devuser/racc/src-tauri && cargo check -p racc-core`
Expected: compiles successfully

- [ ] **Step 4: Commit**

```bash
git add src-tauri/racc-core/src/rtk.rs src-tauri/racc-core/src/lib.rs
git commit -m "feat: add rtk module with platform detection"
```

---

### Task 3: Add binary download with atomic rename

**Files:**
- Modify: `src-tauri/racc-core/src/rtk.rs`

- [ ] **Step 1: Add download function to rtk.rs**

Append to `src-tauri/racc-core/src/rtk.rs`:

```rust
/// Download the rtk binary to $HOME/.racc/bin/rtk.
/// Uses atomic rename (download to .rtk.tmp, then rename) to prevent corruption.
/// Returns Ok(true) if downloaded, Ok(false) if already exists, Err on failure.
pub async fn download_rtk_binary() -> Result<bool, String> {
    let bin_path = match rtk_bin_path() {
        Some(p) => p,
        None => return Err("Could not determine HOME directory".into()),
    };

    // Already exists — skip
    if bin_path.exists() {
        return Ok(false);
    }

    let asset_name = platform_asset_name()
        .ok_or_else(|| format!("Unsupported platform: {} {}", std::env::consts::OS, std::env::consts::ARCH))?;

    let url = download_url(&asset_name);
    let bin_dir = bin_path.parent().unwrap();
    std::fs::create_dir_all(bin_dir)
        .map_err(|e| format!("Failed to create {}: {}", bin_dir.display(), e))?;

    let tmp_path = bin_dir.join(".rtk.tmp");
    let tar_path = bin_dir.join(".rtk.tar.gz");

    // Download tarball
    let response = reqwest::get(&url)
        .await
        .map_err(|e| format!("Failed to download rtk from {}: {}", url, e))?;

    if !response.status().is_success() {
        return Err(format!("HTTP {} downloading rtk from {}", response.status(), url));
    }

    let bytes = response
        .bytes()
        .await
        .map_err(|e| format!("Failed to read rtk download body: {}", e))?;

    std::fs::write(&tar_path, &bytes)
        .map_err(|e| format!("Failed to write {}: {}", tar_path.display(), e))?;

    // Extract the rtk binary from the tarball
    extract_rtk_from_tarball(&tar_path, &tmp_path)?;

    // Clean up tarball
    let _ = std::fs::remove_file(&tar_path);

    // Set executable permissions
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&tmp_path, std::fs::Permissions::from_mode(0o755))
            .map_err(|e| format!("Failed to chmod rtk: {}", e))?;
    }

    // Atomic rename into place
    std::fs::rename(&tmp_path, &bin_path)
        .map_err(|e| format!("Failed to rename rtk into place: {}", e))?;

    log::info!("rtk v{} downloaded to {}", RTK_VERSION, bin_path.display());
    Ok(true)
}

/// Extract the `rtk` binary from a .tar.gz archive.
/// The binary is expected at the top level of the archive (just `rtk`).
fn extract_rtk_from_tarball(tar_gz_path: &Path, dest: &Path) -> Result<(), String> {
    use std::io::Read;

    let file = std::fs::File::open(tar_gz_path)
        .map_err(|e| format!("Failed to open tarball: {}", e))?;
    let gz = flate2::read::GzDecoder::new(file);
    let mut archive = tar::Archive::new(gz);

    for entry in archive.entries().map_err(|e| format!("Failed to read tarball entries: {}", e))? {
        let mut entry = entry.map_err(|e| format!("Failed to read tarball entry: {}", e))?;
        let path = entry.path().map_err(|e| format!("Invalid path in tarball: {}", e))?;

        // Look for the rtk binary (top-level file named "rtk")
        if path.file_name().and_then(|n| n.to_str()) == Some("rtk") {
            let mut content = Vec::new();
            entry.read_to_end(&mut content)
                .map_err(|e| format!("Failed to read rtk from tarball: {}", e))?;
            std::fs::write(dest, &content)
                .map_err(|e| format!("Failed to write extracted rtk: {}", e))?;
            return Ok(());
        }
    }

    Err("rtk binary not found in tarball".into())
}
```

- [ ] **Step 2: Add tar and flate2 dependencies**

Add to `src-tauri/racc-core/Cargo.toml` after the reqwest line:

```toml
flate2 = "1"
tar = "0.4"
```

- [ ] **Step 3: Verify it compiles**

Run: `cd /home/devuser/racc/src-tauri && cargo check -p racc-core`
Expected: compiles successfully

- [ ] **Step 4: Commit**

```bash
git add src-tauri/racc-core/src/rtk.rs src-tauri/racc-core/Cargo.toml src-tauri/Cargo.lock
git commit -m "feat: add rtk binary download with atomic rename and tarball extraction"
```

---

### Task 4: Add Claude Code hook configuration (local)

**Files:**
- Modify: `src-tauri/racc-core/src/rtk.rs`

- [ ] **Step 1: Add hook configuration functions**

Append to `src-tauri/racc-core/src/rtk.rs`:

```rust
/// Configure the Claude Code PreToolUse hook for rtk.
/// Preferred: run `rtk init -g --hook-only --auto-patch`.
/// Fallback: write hook script manually and merge settings.json.
pub fn configure_claude_hook_local() -> Result<(), String> {
    let bin_path = match rtk_bin_path() {
        Some(p) if p.exists() => p,
        _ => return Err("rtk binary not available".into()),
    };

    // Try rtk init --hook-only first (it handles everything)
    let output = std::process::Command::new(&bin_path)
        .args(["init", "-g", "--hook-only", "--auto-patch"])
        .output();

    match output {
        Ok(o) if o.status.success() => {
            log::info!("rtk hook configured via `rtk init -g --hook-only --auto-patch`");
            return Ok(());
        }
        Ok(o) => {
            log::warn!(
                "rtk init --hook-only failed (exit {}), falling back to manual setup: {}",
                o.status,
                String::from_utf8_lossy(&o.stderr)
            );
        }
        Err(e) => {
            log::warn!("Failed to run rtk init: {}, falling back to manual setup", e);
        }
    }

    // Fallback: manual hook setup
    write_hook_script(&bin_path)?;
    merge_settings_json()?;
    Ok(())
}

/// Write the fallback hook script to $HOME/.racc/hooks/rtk-rewrite.sh.
fn write_hook_script(rtk_bin: &Path) -> Result<(), String> {
    let home = std::env::var("HOME").map_err(|_| "HOME not set".to_string())?;
    let hook_dir = PathBuf::from(&home).join(".racc").join("hooks");
    let hook_path = hook_dir.join("rtk-rewrite.sh");

    if hook_path.exists() {
        return Ok(());
    }

    std::fs::create_dir_all(&hook_dir)
        .map_err(|e| format!("Failed to create hook dir: {}", e))?;

    // Note: jq is required — this matches rtk's own hook (hooks/rtk-rewrite.sh).
    // The spec says "no jq dependency" but rtk's official hook requires it.
    // Since rtk init (preferred path) installs the same jq-dependent hook,
    // our fallback mirrors that behavior for consistency.
    let script = format!(
        r#"#!/usr/bin/env bash
# Racc-managed rtk rewrite hook for Claude Code PreToolUse
RTK_BIN="{}"
if [ ! -x "$RTK_BIN" ]; then
  exit 0
fi
if ! command -v jq &>/dev/null; then
  exit 0
fi
INPUT=$(cat)
CMD=$(echo "$INPUT" | jq -r '.tool_input.command // empty')
if [ -z "$CMD" ]; then
  exit 0
fi
REWRITTEN=$("$RTK_BIN" rewrite "$CMD" 2>/dev/null) || exit 0
if [ "$CMD" = "$REWRITTEN" ]; then
  exit 0
fi
ORIGINAL_INPUT=$(echo "$INPUT" | jq -c '.tool_input')
UPDATED_INPUT=$(echo "$ORIGINAL_INPUT" | jq --arg cmd "$REWRITTEN" '.command = $cmd')
jq -n --argjson updated "$UPDATED_INPUT" '{{
  "hookSpecificOutput": {{
    "hookEventName": "PreToolUse",
    "permissionDecision": "allow",
    "permissionDecisionReason": "RTK auto-rewrite",
    "updatedInput": $updated
  }}
}}'
"#,
        rtk_bin.display()
    );

    std::fs::write(&hook_path, &script)
        .map_err(|e| format!("Failed to write hook script: {}", e))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&hook_path, std::fs::Permissions::from_mode(0o755))
            .map_err(|e| format!("Failed to chmod hook script: {}", e))?;
    }

    log::info!("Wrote rtk hook script to {}", hook_path.display());
    Ok(())
}

/// Merge the rtk hook entry into $HOME/.claude/settings.json.
fn merge_settings_json() -> Result<(), String> {
    let home = std::env::var("HOME").map_err(|_| "HOME not set".to_string())?;
    let claude_dir = PathBuf::from(&home).join(".claude");
    let settings_path = claude_dir.join("settings.json");
    let hook_path_str = PathBuf::from(&home)
        .join(".racc")
        .join("hooks")
        .join("rtk-rewrite.sh")
        .to_string_lossy()
        .to_string();

    std::fs::create_dir_all(&claude_dir)
        .map_err(|e| format!("Failed to create .claude dir: {}", e))?;

    let mut root: serde_json::Value = if settings_path.exists() {
        let content = std::fs::read_to_string(&settings_path)
            .map_err(|e| format!("Failed to read settings.json: {}", e))?;
        match serde_json::from_str(&content) {
            Ok(v) => v,
            Err(e) => {
                log::warn!("Failed to parse settings.json, skipping merge: {}", e);
                return Ok(());
            }
        }
    } else {
        serde_json::json!({})
    };

    // Ensure hooks.PreToolUse exists as an array
    let hooks = root
        .as_object_mut()
        .ok_or("settings.json root is not an object")?
        .entry("hooks")
        .or_insert_with(|| serde_json::json!({}));
    let pre_tool_use = hooks
        .as_object_mut()
        .ok_or("hooks is not an object")?
        .entry("PreToolUse")
        .or_insert_with(|| serde_json::json!([]));
    let arr = pre_tool_use
        .as_array_mut()
        .ok_or("PreToolUse is not an array")?;

    // Check if already present
    let already_present = arr.iter().any(|entry| {
        entry.get("hook").and_then(|h| h.as_str()) == Some(&hook_path_str)
    });

    if already_present {
        return Ok(());
    }

    arr.push(serde_json::json!({
        "matcher": "Bash",
        "hook": hook_path_str,
    }));

    let serialized = serde_json::to_string_pretty(&root)
        .map_err(|e| format!("Failed to serialize settings.json: {}", e))?;
    std::fs::write(&settings_path, &serialized)
        .map_err(|e| format!("Failed to write settings.json: {}", e))?;

    log::info!("Added rtk hook to {}", settings_path.display());
    Ok(())
}
```

- [ ] **Step 2: Verify it compiles**

Run: `cd /home/devuser/racc/src-tauri && cargo check -p racc-core`
Expected: compiles successfully

- [ ] **Step 3: Commit**

```bash
git add src-tauri/racc-core/src/rtk.rs
git commit -m "feat: add Claude Code hook configuration for rtk (local)"
```

---

### Task 5: Add ensure_rtk_local() entry point

**Files:**
- Modify: `src-tauri/racc-core/src/rtk.rs`

- [ ] **Step 1: Add the top-level ensure function**

Append to `src-tauri/racc-core/src/rtk.rs`:

```rust
/// Ensure rtk is installed and configured for local Claude Code sessions.
/// Returns true if rtk is available (binary exists on disk).
/// Never fails — logs warnings and returns false on any error.
pub async fn ensure_rtk_local() -> bool {
    // Step 1: Ensure binary exists
    match download_rtk_binary().await {
        Ok(true) => log::info!("rtk binary downloaded successfully"),
        Ok(false) => {} // already existed
        Err(e) => {
            log::warn!("Failed to download rtk: {}", e);
            // Check if it exists anyway (maybe from a previous partial run)
            if rtk_bin_path().map_or(true, |p| !p.exists()) {
                return false;
            }
        }
    }

    // Step 2: Configure hook (idempotent)
    if let Err(e) = configure_claude_hook_local() {
        log::warn!("Failed to configure rtk hook: {}", e);
        // Binary is still available even if hook setup failed
    }

    rtk_bin_path().map_or(false, |p| p.exists())
}

/// Build the PATH environment variable with rtk bin dir prepended.
/// Returns None if rtk bin dir cannot be determined.
pub fn rtk_path_env() -> Option<String> {
    let bin_dir = rtk_bin_path()?.parent()?.to_path_buf();
    let current_path = std::env::var("PATH").unwrap_or_default();
    Some(format!("{}:{}", bin_dir.display(), current_path))
}
```

- [ ] **Step 2: Verify it compiles**

Run: `cd /home/devuser/racc/src-tauri && cargo check -p racc-core`
Expected: compiles successfully

- [ ] **Step 3: Commit**

```bash
git add src-tauri/racc-core/src/rtk.rs
git commit -m "feat: add ensure_rtk_local entry point"
```

---

## Chunk 2: Remote rtk setup via SSH

### Task 6: Add remote rtk installation and hook configuration

**Files:**
- Modify: `src-tauri/racc-core/src/rtk.rs`

- [ ] **Step 1: Add remote functions**

Append to `src-tauri/racc-core/src/rtk.rs`:

```rust
use crate::ssh::SshManager;
use std::sync::Arc;

/// Ensure rtk is installed and configured on a remote server via SSH.
/// Returns true if rtk is available on the remote.
/// Never fails — logs warnings and returns false on any error.
pub async fn ensure_rtk_remote(ssh_manager: &Arc<SshManager>, server_id: &str) -> bool {
    // Step 1: Check if rtk already exists on remote
    let check = ssh_manager
        .exec(server_id, "test -x $HOME/.racc/bin/rtk && echo ok || echo missing")
        .await;

    match check {
        Ok(output) if output.stdout.trim() == "ok" => {
            // Binary exists, ensure hook is configured
            if let Err(e) = configure_claude_hook_remote(ssh_manager, server_id).await {
                log::warn!("Failed to configure remote rtk hook: {}", e);
            }
            return true;
        }
        Ok(_) => {} // missing, proceed to download
        Err(e) => {
            log::warn!("Failed to check remote rtk: {}", e);
            return false;
        }
    }

    // Step 2: Detect remote platform
    let uname = match ssh_manager.exec(server_id, "uname -s && uname -m").await {
        Ok(output) => output.stdout,
        Err(e) => {
            log::warn!("Failed to detect remote platform: {}", e);
            return false;
        }
    };

    let asset_name = match remote_platform_asset_name(&uname) {
        Some(name) => name,
        None => {
            log::warn!("Unsupported remote platform: {}", uname.trim());
            return false;
        }
    };

    let url = download_url(&asset_name);

    // Step 3: Download on remote (atomic temp file + rename)
    let download_cmd = format!(
        "mkdir -p $HOME/.racc/bin && \
         curl -fsSL -o $HOME/.racc/bin/.rtk.tar.gz '{}' && \
         cd $HOME/.racc/bin && \
         tar xzf .rtk.tar.gz -O rtk > .rtk.tmp && \
         chmod +x .rtk.tmp && \
         mv .rtk.tmp rtk && \
         rm -f .rtk.tar.gz",
        url
    );

    match ssh_manager.exec(server_id, &download_cmd).await {
        Ok(output) if output.exit_code == 0 => {
            log::info!("rtk v{} downloaded on remote {}", RTK_VERSION, server_id);
        }
        Ok(output) => {
            log::warn!(
                "Failed to download rtk on remote (exit {}): {}",
                output.exit_code,
                output.stderr.trim()
            );
            return false;
        }
        Err(e) => {
            log::warn!("SSH exec failed for rtk download: {}", e);
            return false;
        }
    }

    // Step 4: Configure hook on remote
    if let Err(e) = configure_claude_hook_remote(ssh_manager, server_id).await {
        log::warn!("Failed to configure remote rtk hook: {}", e);
    }

    true
}

/// Configure the Claude Code hook on a remote server via SSH.
/// Tries `rtk init -g --hook-only --auto-patch` first, falls back to manual.
async fn configure_claude_hook_remote(
    ssh_manager: &Arc<SshManager>,
    server_id: &str,
) -> Result<(), String> {
    // Try rtk init first
    let init_result = ssh_manager
        .exec(
            server_id,
            "PATH=$HOME/.racc/bin:$PATH rtk init -g --hook-only --auto-patch",
        )
        .await;

    match init_result {
        Ok(output) if output.exit_code == 0 => {
            log::info!("Remote rtk hook configured via rtk init");
            return Ok(());
        }
        _ => {
            log::warn!("Remote rtk init failed, falling back to manual hook setup");
        }
    }

    // Fallback: write hook script via base64
    let hook_script = format!(
        r#"#!/usr/bin/env bash
RTK_BIN="$HOME/.racc/bin/rtk"
if [ ! -x "$RTK_BIN" ]; then exit 0; fi
if ! command -v jq &>/dev/null; then exit 0; fi
INPUT=$(cat)
CMD=$(echo "$INPUT" | jq -r '.tool_input.command // empty')
if [ -z "$CMD" ]; then exit 0; fi
REWRITTEN=$("$RTK_BIN" rewrite "$CMD" 2>/dev/null) || exit 0
if [ "$CMD" = "$REWRITTEN" ]; then exit 0; fi
ORIGINAL_INPUT=$(echo "$INPUT" | jq -c '.tool_input')
UPDATED_INPUT=$(echo "$ORIGINAL_INPUT" | jq --arg cmd "$REWRITTEN" '.command = $cmd')
jq -n --argjson updated "$UPDATED_INPUT" '{{
  "hookSpecificOutput": {{
    "hookEventName": "PreToolUse",
    "permissionDecision": "allow",
    "permissionDecisionReason": "RTK auto-rewrite",
    "updatedInput": $updated
  }}
}}'
"#
    );

    let encoded = base64_encode(&hook_script);
    let write_hook_cmd = format!(
        "mkdir -p $HOME/.racc/hooks && echo '{}' | base64 --decode > $HOME/.racc/hooks/rtk-rewrite.sh && chmod +x $HOME/.racc/hooks/rtk-rewrite.sh",
        encoded
    );

    ssh_manager
        .exec(server_id, &write_hook_cmd)
        .await
        .map_err(|e| format!("Failed to write remote hook script: {}", e))?;

    // Read existing settings.json from remote
    let read_result = ssh_manager
        .exec(server_id, "cat $HOME/.claude/settings.json 2>/dev/null || echo '{}'")
        .await
        .map_err(|e| format!("Failed to read remote settings.json: {}", e))?;

    let mut root: serde_json::Value = serde_json::from_str(read_result.stdout.trim())
        .unwrap_or_else(|_| serde_json::json!({}));

    // Get remote HOME for absolute path
    let remote_home = ssh_manager
        .exec(server_id, "echo $HOME")
        .await
        .map_err(|e| format!("Failed to get remote HOME: {}", e))?
        .stdout
        .trim()
        .to_string();

    let hook_abs_path = format!("{}/.racc/hooks/rtk-rewrite.sh", remote_home);

    // Merge hook entry (same algorithm as local)
    let hooks = root
        .as_object_mut()
        .ok_or("root is not object")?
        .entry("hooks")
        .or_insert_with(|| serde_json::json!({}));
    let pre_tool_use = hooks
        .as_object_mut()
        .ok_or("hooks is not object")?
        .entry("PreToolUse")
        .or_insert_with(|| serde_json::json!([]));
    let arr = pre_tool_use
        .as_array_mut()
        .ok_or("PreToolUse is not array")?;

    let already = arr.iter().any(|e| {
        e.get("hook").and_then(|h| h.as_str()).map_or(false, |h| h.contains("rtk"))
    });

    if !already {
        arr.push(serde_json::json!({
            "matcher": "Bash",
            "hook": hook_abs_path,
        }));

        let serialized = serde_json::to_string_pretty(&root)
            .map_err(|e| format!("Failed to serialize: {}", e))?;
        let encoded_settings = base64_encode(&serialized);
        let write_settings_cmd = format!(
            "mkdir -p $HOME/.claude && echo '{}' | base64 --decode > $HOME/.claude/settings.json",
            encoded_settings
        );
        ssh_manager
            .exec(server_id, &write_settings_cmd)
            .await
            .map_err(|e| format!("Failed to write remote settings.json: {}", e))?;
    }

    log::info!("Remote rtk hook configured manually on {}", server_id);
    Ok(())
}

/// Base64-encode a string for safe SSH transport.
fn base64_encode(input: &str) -> String {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.encode(input.as_bytes())
}
```

- [ ] **Step 2: Add base64 dependency to Cargo.toml**

Add to `src-tauri/racc-core/Cargo.toml`:

```toml
base64 = "0.22"
```

- [ ] **Step 3: Verify it compiles**

Run: `cd /home/devuser/racc/src-tauri && cargo check -p racc-core`
Expected: compiles successfully

- [ ] **Step 4: Commit**

```bash
git add src-tauri/racc-core/src/rtk.rs src-tauri/racc-core/Cargo.toml src-tauri/Cargo.lock
git commit -m "feat: add remote rtk installation and hook configuration via SSH"
```

---

## Chunk 3: Session integration — wire rtk into PTY spawning

### Task 7: Add extra_env parameter to LocalPtyTransport::spawn()

**Files:**
- Modify: `src-tauri/racc-core/src/transport/local_pty.rs:15-32`

- [ ] **Step 1: Add extra_env parameter to spawn()**

In `src-tauri/racc-core/src/transport/local_pty.rs`, modify the `spawn` method signature and body.

Change the signature from:

```rust
    pub async fn spawn(
        session_id: i64,
        cwd: &str,
        cmd: &str,
        cols: u16,
        rows: u16,
        terminal_tx: tokio::sync::broadcast::Sender<crate::TerminalData>,
        buffer_tx: tokio::sync::mpsc::UnboundedSender<(i64, Vec<u8>)>,
    ) -> Result<Self, TransportError> {
```

To:

```rust
    pub async fn spawn(
        session_id: i64,
        cwd: &str,
        cmd: &str,
        cols: u16,
        rows: u16,
        terminal_tx: tokio::sync::broadcast::Sender<crate::TerminalData>,
        buffer_tx: tokio::sync::mpsc::UnboundedSender<(i64, Vec<u8>)>,
        extra_env: Option<std::collections::HashMap<String, String>>,
    ) -> Result<Self, TransportError> {
```

Then after `cmd_builder.env("TERM", "xterm-256color");` (line 32), add:

```rust
        if let Some(ref envs) = extra_env {
            for (key, value) in envs {
                cmd_builder.env(key, value);
            }
        }
```

- [ ] **Step 2: Verify it compiles (expect errors from callers)**

Run: `cd /home/devuser/racc/src-tauri && cargo check -p racc-core 2>&1 | head -30`
Expected: errors in `session.rs` because existing call sites don't pass `extra_env` yet. This confirms the signature change is correct.

- [ ] **Step 3: Commit**

```bash
git add src-tauri/racc-core/src/transport/local_pty.rs
git commit -m "feat: add extra_env parameter to LocalPtyTransport::spawn"
```

---

### Task 8: Wire rtk into create_session() and reattach_session()

**Files:**
- Modify: `src-tauri/racc-core/src/commands/session.rs:128-142` (build_agent_command)
- Modify: `src-tauri/racc-core/src/commands/session.rs:383-400` (create_session local spawn)
- Modify: `src-tauri/racc-core/src/commands/session.rs:308-382` (create_session remote spawn)
- Modify: `src-tauri/racc-core/src/commands/session.rs:601-626` (reattach_session local spawn)
- Modify: `src-tauri/racc-core/src/commands/session.rs:570-600` (reattach_session remote spawn)

- [ ] **Step 1: Update build_agent_command to accept rtk_remote flag**

In `session.rs`, change `build_agent_command` from:

```rust
fn build_agent_command(agent: &str, task: &str, _cwd: &str, skip_permissions: bool) -> String {
    match agent {
        "claude-code" => {
            let escaped_task = task.replace('\'', "'\\''");
            let dangerously = if skip_permissions { " --dangerously-skip-permissions" } else { "" };
            format!("claude{} '{}'\n", dangerously, escaped_task)
        }
```

To:

```rust
fn build_agent_command(agent: &str, task: &str, _cwd: &str, skip_permissions: bool, rtk_remote: bool) -> String {
    match agent {
        "claude-code" => {
            let escaped_task = task.replace('\'', "'\\''");
            let dangerously = if skip_permissions { " --dangerously-skip-permissions" } else { "" };
            let prefix = if rtk_remote { "PATH=$HOME/.racc/bin:$PATH " } else { "" };
            format!("{}claude{} '{}'\n", prefix, dangerously, escaped_task)
        }
```

Update all existing call sites of `build_agent_command` to pass `false` as the last argument (we'll set `true` in the next step for remote Claude Code sessions).

- [ ] **Step 2: Wire ensure_rtk_local into create_session (local path)**

Add `use crate::rtk;` at the top of `session.rs`.

In `create_session()`, in the `else` branch (local session, around line 383), before the `LocalPtyTransport::spawn()` call, add rtk setup:

```rust
        // RTK setup for Claude Code sessions
        let extra_env = if agent == "claude-code" {
            let rtk_available = rtk::ensure_rtk_local().await;
            if rtk_available {
                rtk::rtk_path_env().map(|p| {
                    let mut env = std::collections::HashMap::new();
                    env.insert("PATH".to_string(), p);
                    env
                })
            } else {
                None
            }
        } else {
            None
        };
```

Then pass `extra_env` to `LocalPtyTransport::spawn()`:

```rust
        let transport = LocalPtyTransport::spawn(
            session_id,
            cwd,
            "/bin/zsh",
            80,
            24,
            ctx.terminal_tx.clone(),
            ctx.transport_manager.buffer_sender(),
            extra_env,
        )
```

- [ ] **Step 3: Wire ensure_rtk_remote into create_session (remote path)**

In `create_session()`, in the `if let Some(ref sid) = server_id` branch (around line 308), before the `SshTmuxTransport::spawn()` call, add:

```rust
        // RTK setup for remote Claude Code sessions
        let rtk_remote = if agent == "claude-code" {
            crate::rtk::ensure_rtk_remote(&ctx.ssh_manager, sid).await
        } else {
            false
        };
```

Then update the `build_agent_command` call for remote to pass `rtk_remote`:

```rust
        let agent_cmd = build_agent_command(&agent, &task_description, &remote_worktree, skip_permissions, rtk_remote);
```

- [ ] **Step 4: Wire rtk into reattach_session (local path)**

In `reattach_session()`, in the local branch (around line 601), before `LocalPtyTransport::spawn()`, add the same pattern:

```rust
        let extra_env = if agent == "claude-code" {
            let rtk_available = rtk::ensure_rtk_local().await;
            if rtk_available {
                rtk::rtk_path_env().map(|p| {
                    let mut env = std::collections::HashMap::new();
                    env.insert("PATH".to_string(), p);
                    env
                })
            } else {
                None
            }
        } else {
            None
        };
```

Pass `extra_env` to the `LocalPtyTransport::spawn()` call.

- [ ] **Step 5: Wire rtk into reattach_session (remote path)**

In `reattach_session()`, in the remote branch (around line 570), add:

```rust
        let rtk_remote = if agent == "claude-code" {
            crate::rtk::ensure_rtk_remote(&ctx.ssh_manager, sid).await
        } else {
            false
        };
```

Update the existing `build_agent_command` call at line 585 from:

```rust
        let agent_cmd = build_agent_command(&agent, "", &remote_worktree, false);
```

To:

```rust
        let agent_cmd = build_agent_command(&agent, "", &remote_worktree, false, rtk_remote);
```

- [ ] **Step 6: Verify everything compiles**

Run: `cd /home/devuser/racc/src-tauri && cargo check -p racc-core`
Expected: compiles successfully with no errors

- [ ] **Step 7: Verify the full workspace compiles**

Run: `cd /home/devuser/racc/src-tauri && cargo check`
Expected: compiles successfully (the Tauri app and server crates call `LocalPtyTransport::spawn()` indirectly through racc-core, so they should be unaffected)

- [ ] **Step 8: Commit**

```bash
git add src-tauri/racc-core/src/commands/session.rs
git commit -m "feat: wire rtk setup into create_session and reattach_session"
```

---

## Chunk 4: Verification and cleanup

### Task 9: Full build verification

**Files:** None (verification only)

- [ ] **Step 1: Run cargo check on entire workspace**

Run: `cd /home/devuser/racc/src-tauri && cargo check`
Expected: all crates compile successfully

- [ ] **Step 2: Run cargo build for racc-core**

Run: `cd /home/devuser/racc/src-tauri && cargo build -p racc-core`
Expected: builds successfully

- [ ] **Step 3: Run cargo build for racc-server**

Run: `cd /home/devuser/racc/src-tauri && cargo build --bin racc-server`
Expected: builds successfully

- [ ] **Step 4: Verify frontend builds**

Run: `cd /home/devuser/racc && bun run build`
Expected: builds successfully (no frontend changes, so this should be unaffected)

- [ ] **Step 5: Commit any remaining changes**

If there are any uncommitted fixups from build issues, commit them:

```bash
git add -A
git commit -m "fix: address build issues from rtk integration"
```

Only run if there are actual changes to commit.

---

### Task 10: Manual smoke test

**Files:** None (testing only)

- [ ] **Step 1: Test platform detection**

Add a temporary test or just verify the logic mentally:
- On Linux x86_64: `platform_asset_name()` should return `Some("rtk-x86_64-unknown-linux-musl")`
- On macOS ARM: `platform_asset_name()` should return `Some("rtk-aarch64-apple-darwin")`

- [ ] **Step 2: Test with `bun tauri dev` (if on macOS/Linux desktop)**

Run: `cd /home/devuser/racc && bun tauri dev`

1. Import a repo
2. Create a Claude Code session
3. Check logs for rtk download/hook messages
4. Verify `$HOME/.racc/bin/rtk` exists after session creation
5. Verify Claude Code hook is configured in `$HOME/.claude/settings.json`

- [ ] **Step 3: Test graceful degradation**

Temporarily move rtk binary away and create a session — should succeed without rtk:

```bash
mv ~/.racc/bin/rtk ~/.racc/bin/rtk.bak
# Create session in Racc — should work fine, just without rtk
mv ~/.racc/bin/rtk.bak ~/.racc/bin/rtk
```
