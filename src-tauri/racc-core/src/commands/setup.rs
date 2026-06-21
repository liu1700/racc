//! One-click remote server provisioning.
//!
//! `setup_server` connects to a saved server over SSH and brings it to a state
//! where Racc can run Claude Code sessions: it detects the OS / package manager /
//! privilege level, then checks (and installs, where possible) the tools a remote
//! session needs — `curl`, `git`, `tmux`, `jq`, the `claude` binary, and RTK.
//!
//! It returns a per-step report so the UI can render a checklist instead of a
//! single pass/fail. System packages are installed via the detected package
//! manager using passwordless sudo (or directly when running as root); Claude
//! Code is installed via the official installer into `~/.local/bin` (no sudo),
//! with an npm fallback.

use std::sync::Arc;

use rusqlite::params;
use serde::Serialize;

use crate::AppContext;
use crate::commands::server::get_server_by_id;
use crate::error::CoreError;
use crate::ssh::SshManager;

/// Prepended to every probe / install command. `exec` runs a non-interactive,
/// non-login shell whose PATH usually omits user-local bin dirs, so tools like
/// the `claude` binary (installed to `~/.local/bin`) would otherwise be invisible.
const PATH_PREFIX: &str = "export PATH=\"$HOME/.local/bin:$HOME/bin:$HOME/.npm-global/bin:/opt/homebrew/bin:/usr/local/bin:/usr/sbin:/sbin:/usr/bin:/bin:$PATH\"; ";

#[derive(Debug, Clone, Serialize)]
pub struct SetupStep {
    pub key: String,
    pub label: String,
    /// "ok" (already present) | "installed" (we installed it) | "failed" | "skipped"
    pub status: String,
    pub detail: Option<String>,
}

impl SetupStep {
    fn new(key: &str, label: &str, status: &str, detail: Option<String>) -> Self {
        Self {
            key: key.into(),
            label: label.into(),
            status: status.into(),
            detail,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct SetupReport {
    pub server_id: String,
    pub ok: bool,
    pub steps: Vec<SetupStep>,
}

/// Connect to a server and provision it for Claude Code sessions.
/// Persists `setup_status` ("ready" / "failed") and the JSON step report.
pub async fn setup_server(ctx: &AppContext, server_id: String) -> Result<SetupReport, CoreError> {
    let server = {
        let conn = ctx.db.lock().map_err(|e| CoreError::Other(e.to_string()))?;
        get_server_by_id(&conn, &server_id)?
    };

    ctx.ssh_manager
        .connect(
            &server_id,
            &server.host,
            server.port as u16,
            &server.username,
            &server.auth_method,
            server.key_path.as_deref(),
        )
        .await
        .map_err(CoreError::Ssh)?;

    let steps = run_provision(&ctx.ssh_manager, &server_id).await;

    // Best-effort disconnect; the connection is re-established when a session starts.
    let _ = ctx.ssh_manager.disconnect(&server_id).await;

    // RTK is advisory (it's retried on session start), so it never fails setup.
    let ok = steps
        .iter()
        .filter(|s| s.key != "rtk")
        .all(|s| s.status != "failed");

    {
        let conn = ctx.db.lock().map_err(|e| CoreError::Other(e.to_string()))?;
        let now = chrono::Utc::now().to_rfc3339();
        let details = serde_json::to_string(&steps).unwrap_or_default();
        conn.execute(
            "UPDATE servers SET setup_status=?1, setup_details=?2, updated_at=?3 WHERE id=?4",
            params![if ok { "ready" } else { "failed" }, details, now, server_id],
        )?;
    }

    Ok(SetupReport {
        server_id,
        ok,
        steps,
    })
}

async fn run_provision(ssh: &Arc<SshManager>, sid: &str) -> Vec<SetupStep> {
    let mut steps = Vec::new();

    // ── 1. OS / arch ────────────────────────────────────────────────
    let uname = ssh
        .exec(sid, "uname -s; uname -m")
        .await
        .map(|o| o.stdout.split_whitespace().collect::<Vec<_>>().join(" "))
        .unwrap_or_default();
    let os_detail = if uname.is_empty() { "unknown".to_string() } else { uname.clone() };
    steps.push(SetupStep::new("os", "Operating system", "ok", Some(os_detail)));

    // ── 2. package manager ──────────────────────────────────────────
    let pm = ssh
        .exec(
            sid,
            "for pm in apt-get dnf yum pacman apk zypper brew; do if command -v $pm >/dev/null 2>&1; then echo $pm; break; fi; done",
        )
        .await
        .map(|o| o.stdout.trim().to_string())
        .unwrap_or_default();

    // ── 3. privileges ───────────────────────────────────────────────
    let uid = ssh
        .exec(sid, "id -u")
        .await
        .map(|o| o.stdout.trim().to_string())
        .unwrap_or_default();
    let is_root = uid == "0";
    let has_sudo = if is_root {
        true
    } else {
        ssh.exec(sid, "sudo -n true >/dev/null 2>&1 && echo yes || echo no")
            .await
            .map(|o| o.stdout.contains("yes"))
            .unwrap_or(false)
    };
    // Use `sudo -n` so a missing-password prompt fails fast instead of hanging
    // (exec has no PTY to answer a prompt).
    let sudo = if is_root || !has_sudo { "" } else { "sudo -n " };
    let can_install_system = !pm.is_empty() && (is_root || has_sudo);

    let priv_label = if is_root {
        "root"
    } else if has_sudo {
        "sudo (passwordless)"
    } else {
        "no sudo"
    };
    let pm_label = if pm.is_empty() { "none detected" } else { pm.as_str() };
    steps.push(SetupStep::new(
        "pkg",
        "Package manager",
        if pm.is_empty() { "failed" } else { "ok" },
        Some(format!("{} · {}", pm_label, priv_label)),
    ));

    // ── 4. system tools: curl, git, tmux, jq ────────────────────────
    let tools = [("curl", "curl"), ("git", "git"), ("tmux", "tmux"), ("jq", "jq")];

    let mut present: Vec<(&str, String)> = Vec::new();
    let mut missing: Vec<&str> = Vec::new();
    for (key, bin) in tools {
        match detect_tool(ssh, sid, bin).await {
            Ok(Some(v)) => present.push((key, v)),
            _ => missing.push(bin),
        }
    }

    let mut install_err: Option<String> = None;
    if !missing.is_empty() {
        if can_install_system {
            match build_install_cmd(&pm, sudo, &missing) {
                Some(cmd) => match ssh.exec(sid, &cmd).await {
                    Ok(o) if o.exit_code == 0 => {}
                    Ok(o) => install_err = Some(trunc(&o.stderr)),
                    Err(e) => install_err = Some(e),
                },
                None => install_err = Some(format!("don't know how to install on '{}'", pm)),
            }
        } else if pm.is_empty() {
            install_err = Some("no supported package manager found".into());
        } else {
            install_err = Some("no root / passwordless-sudo to install system packages".into());
        }
    }

    for (key, bin) in tools {
        if let Some((_, v)) = present.iter().find(|(k, _)| *k == key) {
            steps.push(SetupStep::new(key, bin, "ok", Some(v.clone())));
        } else {
            // Was missing — re-detect to see if the install took.
            match detect_tool(ssh, sid, bin).await {
                Ok(Some(v)) => steps.push(SetupStep::new(key, bin, "installed", Some(v))),
                _ => steps.push(SetupStep::new(
                    key,
                    bin,
                    "failed",
                    install_err.clone().or_else(|| Some("not installed".into())),
                )),
            }
        }
    }

    // ── 5. Claude Code ──────────────────────────────────────────────
    steps.push(ensure_claude(ssh, sid).await);

    // ── 6. RTK (advisory; auto-retried when a session starts) ───────
    let rtk_ok = crate::rtk::ensure_rtk_remote(ssh, sid).await;
    steps.push(SetupStep::new(
        "rtk",
        "RTK (command rewriter)",
        if rtk_ok { "ok" } else { "skipped" },
        if rtk_ok {
            None
        } else {
            Some("optional — will retry on session start".into())
        },
    ));

    steps
}

/// Ensure the `claude` binary is available, installing it if needed.
async fn ensure_claude(ssh: &Arc<SshManager>, sid: &str) -> SetupStep {
    if let Ok(Some(v)) = detect_tool(ssh, sid, "claude").await {
        return SetupStep::new("claude", "Claude Code", "ok", Some(v));
    }

    // Official native installer → ~/.local/bin (no sudo).
    let native = ssh
        .exec(
            sid,
            &format!("{}curl -fsSL https://claude.ai/install.sh | bash", PATH_PREFIX),
        )
        .await;
    if matches!(&native, Ok(o) if o.exit_code == 0) {
        if let Ok(Some(v)) = detect_tool(ssh, sid, "claude").await {
            return SetupStep::new("claude", "Claude Code", "installed", Some(v));
        }
    }

    // Fallback: npm global install.
    let has_npm = ssh
        .exec(sid, &format!("{}command -v npm >/dev/null 2>&1", PATH_PREFIX))
        .await
        .map(|o| o.exit_code == 0)
        .unwrap_or(false);
    if has_npm {
        let _ = ssh
            .exec(
                sid,
                &format!("{}npm install -g @anthropic-ai/claude-code", PATH_PREFIX),
            )
            .await;
        if let Ok(Some(v)) = detect_tool(ssh, sid, "claude").await {
            return SetupStep::new("claude", "Claude Code", "installed", Some(v));
        }
    }

    let detail = match &native {
        Ok(o) => trunc(&format!("{}{}", o.stdout, o.stderr)),
        Err(e) => e.clone(),
    };
    SetupStep::new("claude", "Claude Code", "failed", Some(detail))
}

/// Returns `Some(version)` if `bin` is on PATH, `None` otherwise.
async fn detect_tool(
    ssh: &Arc<SshManager>,
    sid: &str,
    bin: &str,
) -> Result<Option<String>, String> {
    // Use a stdout sentinel instead of the exit code: it's robust even if a
    // server omits the SSH ExitStatus message.
    let check = ssh
        .exec(
            sid,
            &format!(
                "{}command -v {} >/dev/null 2>&1 && echo __FOUND__ || echo __MISSING__",
                PATH_PREFIX, bin
            ),
        )
        .await?;
    if !check.stdout.contains("__FOUND__") {
        return Ok(None);
    }
    let ver = ssh
        .exec(sid, &format!("{}{} --version 2>&1 | head -n1", PATH_PREFIX, bin))
        .await?;
    let v = ver.stdout.trim().to_string();
    Ok(Some(if v.is_empty() { "installed".into() } else { v }))
}

/// Build a non-interactive install command for the given package manager.
fn build_install_cmd(pm: &str, sudo: &str, pkgs: &[&str]) -> Option<String> {
    let list = pkgs.join(" ");
    let cmd = match pm {
        // `sudo -n env VAR=val cmd` reliably sets the var through sudo's env reset.
        "apt-get" => format!(
            "{s}apt-get update -y >/dev/null 2>&1; {s}env DEBIAN_FRONTEND=noninteractive apt-get install -y {list}",
            s = sudo,
            list = list
        ),
        "dnf" => format!("{s}dnf install -y {list}", s = sudo, list = list),
        "yum" => format!("{s}yum install -y {list}", s = sudo, list = list),
        "pacman" => format!("{s}pacman -Sy --noconfirm {list}", s = sudo, list = list),
        "apk" => format!("{s}apk add {list}", s = sudo, list = list),
        "zypper" => format!("{s}zypper install -y {list}", s = sudo, list = list),
        // Homebrew refuses to run under sudo.
        "brew" => format!("brew install {list}", list = list),
        _ => return None,
    };
    Some(cmd)
}

/// Trim and cap a command output for use as a step detail (char-boundary safe).
fn trunc(s: &str) -> String {
    let t = s.trim();
    if t.chars().count() > 400 {
        let cut: String = t.chars().take(400).collect();
        format!("{}…", cut)
    } else {
        t.to_string()
    }
}
