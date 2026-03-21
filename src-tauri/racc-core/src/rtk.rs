use std::path::{Path, PathBuf};
use crate::ssh::SshManager;
use std::sync::Arc;

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
    let bin_dir = bin_path.parent()
        .ok_or("rtk bin path has no parent directory")?;
    std::fs::create_dir_all(bin_dir)
        .map_err(|e| format!("Failed to create {}: {}", bin_dir.display(), e))?;

    let tmp_path = bin_dir.join(".rtk.tmp");
    let tar_path = bin_dir.join(".rtk.tar.gz");

    // Download tarball (with 60s timeout)
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(60))
        .build()
        .map_err(|e| format!("Failed to build HTTP client: {}", e))?;
    let response = client.get(&url)
        .send()
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

/// Configure the Claude Code PreToolUse hook for rtk.
/// Preferred: run `rtk init -g --hook-only --auto-patch`.
/// Fallback: write hook script manually and merge settings.json.
pub async fn configure_claude_hook_local() -> Result<(), String> {
    let bin_path = match rtk_bin_path() {
        Some(p) if p.exists() => p,
        _ => return Err("rtk binary not available".into()),
    };

    // Try rtk init --hook-only first (it handles everything)
    // Use spawn_blocking to avoid blocking the tokio executor
    let bin_path_clone = bin_path.clone();
    let init_result = tokio::task::spawn_blocking(move || {
        std::process::Command::new(&bin_path_clone)
            .args(["init", "-g", "--hook-only", "--auto-patch"])
            .output()
    })
    .await
    .map_err(|e| format!("spawn_blocking failed: {}", e))?;

    match init_result {
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
    let script = format!(
        r#"#!/usr/bin/env bash
# Racc-managed rtk rewrite hook for Claude Code PreToolUse
RTK_BIN='{}'
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
    if let Err(e) = configure_claude_hook_local().await {
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
         tar xzf .rtk.tar.gz && mv rtk .rtk.tmp && \
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
    let hook_script = r#"#!/usr/bin/env bash
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
jq -n --argjson updated "$UPDATED_INPUT" '{
  "hookSpecificOutput": {
    "hookEventName": "PreToolUse",
    "permissionDecision": "allow",
    "permissionDecisionReason": "RTK auto-rewrite",
    "updatedInput": $updated
  }
}'
"#;

    let encoded = base64_encode(hook_script);
    let write_hook_cmd = format!(
        "mkdir -p $HOME/.racc/hooks && echo '{}' | base64 --decode > $HOME/.racc/hooks/rtk-rewrite.sh && chmod +x $HOME/.racc/hooks/rtk-rewrite.sh",
        encoded
    );

    let hook_result = ssh_manager
        .exec(server_id, &write_hook_cmd)
        .await
        .map_err(|e| format!("Failed to write remote hook script: {}", e))?;
    if hook_result.exit_code != 0 {
        return Err(format!(
            "Remote hook write failed (exit {}): {}",
            hook_result.exit_code,
            hook_result.stderr.trim()
        ));
    }

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
        e.get("hook").and_then(|h| h.as_str()) == Some(hook_abs_path.as_str())
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
