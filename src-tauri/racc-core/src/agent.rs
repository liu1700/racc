use regex::Regex;
use std::sync::OnceLock;

/// Agent types supported by Racc.
#[derive(Debug, Clone, PartialEq)]
pub enum AgentType {
    ClaudeCode,
    Aider,
    Codex,
    Generic,
}

impl AgentType {
    pub fn from_agent_str(agent: &str) -> Self {
        match agent {
            "claude-code" => Self::ClaudeCode,
            "aider" => Self::Aider,
            "codex" => Self::Codex,
            _ => Self::Generic,
        }
    }
}

/// Strip ANSI escape sequences from raw PTY output bytes.
pub fn strip_ansi(input: &[u8]) -> String {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| {
        Regex::new(r"\x1b\[[0-9;]*[a-zA-Z]|\x1b\][^\x07]*\x07|\x1b[()][AB012]|\x1b\[[\?]?[0-9;]*[hlm]").unwrap()
    });
    let text = String::from_utf8_lossy(input);
    re.replace_all(&text, "").to_string()
}

/// Compiled health detection patterns for an agent type.
pub struct HealthPatterns {
    pub completion: &'static Regex,
    pub error: &'static Regex,
    pub stuck_timeout_secs: u64,
}

impl HealthPatterns {
    pub fn for_agent(agent_type: &AgentType) -> Self {
        match agent_type {
            AgentType::ClaudeCode => {
                static COMPLETION: OnceLock<Regex> = OnceLock::new();
                static ERROR: OnceLock<Regex> = OnceLock::new();
                Self {
                    completion: COMPLETION.get_or_init(|| {
                        Regex::new(r"(?m)(╭─|╭\u{2500}|\$\s*$|❯\s*$)").unwrap()
                    }),
                    error: ERROR.get_or_init(|| {
                        Regex::new(r"(?m)(^Error:|panicked at|FATAL|SIGTERM|SIGKILL|thread '.*' panicked)").unwrap()
                    }),
                    stuck_timeout_secs: 180,
                }
            }
            AgentType::Aider => {
                static COMPLETION: OnceLock<Regex> = OnceLock::new();
                static ERROR: OnceLock<Regex> = OnceLock::new();
                Self {
                    completion: COMPLETION.get_or_init(|| {
                        Regex::new(r"(?m)>\s*$").unwrap()
                    }),
                    error: ERROR.get_or_init(|| {
                        Regex::new(r"(?m)(Traceback|Error:|Exception:)").unwrap()
                    }),
                    stuck_timeout_secs: 120,
                }
            }
            AgentType::Codex | AgentType::Generic => {
                static COMPLETION: OnceLock<Regex> = OnceLock::new();
                static ERROR: OnceLock<Regex> = OnceLock::new();
                Self {
                    completion: COMPLETION.get_or_init(|| {
                        Regex::new(r"(?m)(\$\s*$|#\s*$|>\s*$)").unwrap()
                    }),
                    error: ERROR.get_or_init(|| {
                        Regex::new(r"(?m)(^Error:|panicked at|FATAL|exit code [1-9])").unwrap()
                    }),
                    stuck_timeout_secs: 300,
                }
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum AgentSignal {
    Idle,
    Completion,
    Error(String),
}

/// Analyze output for health signals. Uses a sliding window (last `window_size` bytes).
/// Checks completion first (prompt at end of buffer), then error patterns.
pub fn analyze_output(output: &[u8], agent_type: &AgentType, window_size: usize) -> AgentSignal {
    let start = if output.len() > window_size { output.len() - window_size } else { 0 };
    let window = &output[start..];
    let text = strip_ansi(window);
    let patterns = HealthPatterns::for_agent(agent_type);

    // Check completion first — prompt at END of buffer takes priority
    if patterns.completion.is_match(&text) {
        let tail_start = if text.len() > 200 {
            let mut i = text.len() - 200;
            while i < text.len() && !text.is_char_boundary(i) { i += 1; }
            i
        } else { 0 };
        let tail = &text[tail_start..];
        if patterns.completion.is_match(tail) {
            return AgentSignal::Completion;
        }
    }

    // Check error — specific patterns to avoid false positives
    if let Some(m) = patterns.error.find(&text) {
        return AgentSignal::Error(m.as_str().to_string());
    }

    AgentSignal::Idle
}

/// PATH prefix that makes `claude` resolvable: ~/.local/bin is where the
/// official installer drops it, and .racc/bin is prepended for RTK when
/// available on a remote server.
fn claude_path_prefix(rtk_remote: bool) -> &'static str {
    if rtk_remote {
        "PATH=$HOME/.racc/bin:$HOME/.local/bin:$PATH "
    } else {
        "PATH=$HOME/.local/bin:$PATH "
    }
}

/// Build the shell command to launch an agent (moved from session.rs).
/// Returns only the launch command — task description is sent separately
/// via PTY write after the agent initializes, avoiding all shell escaping issues.
/// For claude-code, `session_uuid` pins the conversation's session ID
/// (`--session-id <uuid>`) so a later reattach can deterministically resume it
/// with `claude --resume <uuid>` (issue #70).
pub fn build_command(
    agent: &str,
    _cwd: &str,
    skip_permissions: bool,
    rtk_remote: bool,
    session_uuid: Option<&str>,
) -> String {
    match agent {
        "claude-code" => {
            let dangerously = if skip_permissions { " --dangerously-skip-permissions" } else { "" };
            let session_arg = session_uuid
                .map(|u| format!(" --session-id {}", u))
                .unwrap_or_default();
            format!("{}claude{}{}\n", claude_path_prefix(rtk_remote), dangerously, session_arg)
        }
        "aider" => "aider\n".to_string(),
        "codex" => "codex\n".to_string(),
        _ => format!("{}\n", agent),
    }
}

/// Mint the persistent conversation ID for agents that support resuming by
/// session id (claude-code only today). Generated at session-create time,
/// stored in `sessions.agent_session_id`, and consumed by
/// [`build_resume_command`]; agents with no resume-by-id concept get None.
pub fn new_agent_session_id(agent: &str) -> Option<String> {
    match agent {
        "claude-code" => Some(uuid::Uuid::new_v4().to_string()),
        _ => None,
    }
}

/// Build the shell command to bring an agent back after its process died (app
/// restart killed the local PTY, or a remote tmux session is gone). For
/// claude-code with a recorded session id we resume that exact conversation
/// (`--resume <uuid>`); legacy rows (NULL `agent_session_id`) fall back to
/// `--continue`, which picks the most recent conversation in the cwd. Agents
/// with no resume concept are simply relaunched — for them, relaunch IS resume.
pub fn build_resume_command(agent: &str, agent_session_id: Option<&str>, rtk_remote: bool) -> String {
    match agent {
        "claude-code" => {
            let prefix = claude_path_prefix(rtk_remote);
            // Defense-in-depth: the id is interpolated into a shell command
            // (local PTY / remote tmux), so only accept a well-formed UUID —
            // we only ever mint UUIDs, anything else means a tampered DB.
            match agent_session_id.filter(|u| uuid::Uuid::parse_str(u).is_ok()) {
                Some(uuid) => format!("{}claude --resume {}\n", prefix, uuid),
                None => format!("{}claude --continue\n", prefix),
            }
        }
        _ => build_command(agent, "", false, rtk_remote, None),
    }
}

/// Detect that `claude --resume <uuid>` / `claude --continue` failed to find a
/// transcript. Claude prints "No conversation found with session ID: <uuid>"
/// (resume) or "No conversation found to continue" (continue) and exits,
/// leaving a dead shell prompt — which must be surfaced as an Error session,
/// not left as a phantom "Running" one (issue #70). Matches the exact phrases
/// (not just "No conversation found") so a resumed transcript that merely
/// *mentions* the phrase can't false-flag a healthy session.
pub fn is_resume_failure(text: &str) -> bool {
    text.contains("No conversation found with session ID")
        || text.contains("No conversation found to continue")
}

/// Detect Claude Code's first-run "trust this folder" dialog. The injector
/// auto-accepts it (Enter selects the pre-highlighted "Yes, I trust this
/// folder") so a fired task isn't blocked on a manual confirmation, and so the
/// task text isn't typed into the dialog and lost.
pub fn is_trust_dialog(text: &str) -> bool {
    text.contains("trust this folder") || text.contains("Do you trust")
}

/// Detect that an agent has reached its interactive input prompt and is ready to
/// receive a task. For Claude Code we look for the persistent footer hints,
/// which appear only at the real prompt — never in the trust/confirm dialogs
/// (those also render a `❯`, which is why matching on `❯` alone caused the task
/// to be injected into the dialog and dropped).
pub fn is_agent_ready(agent_type: &AgentType, text: &str) -> bool {
    match agent_type {
        AgentType::ClaudeCode => {
            text.contains("for shortcuts")
                || text.contains("shift+tab")
                || text.contains("bypass permissions")
                || text.contains("auto-accept edits")
        }
        _ => {
            text.contains("❯")
                || text.contains("╭")
                || text.ends_with("$ ")
                || text.ends_with("> ")
        }
    }
}

/// Build PTY input to inject a task into an already-running agent.
pub fn inject_task_input(agent_type: &AgentType, task_description: &str) -> Vec<u8> {
    // Use \r (carriage return) to simulate Enter in PTY raw mode.
    // Agent TUIs (Claude Code, etc.) expect \r, not \n.
    match agent_type {
        AgentType::ClaudeCode => format!("{}\r", task_description).into_bytes(),
        AgentType::Aider => format!("/ask {}\r", task_description).into_bytes(),
        AgentType::Codex => format!("{}\r", task_description).into_bytes(),
        AgentType::Generic => format!("{}\r", task_description).into_bytes(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_agent_type_detection() {
        assert_eq!(AgentType::from_agent_str("claude-code"), AgentType::ClaudeCode);
        assert_eq!(AgentType::from_agent_str("aider"), AgentType::Aider);
        assert_eq!(AgentType::from_agent_str("codex"), AgentType::Codex);
        assert_eq!(AgentType::from_agent_str("some-custom-agent"), AgentType::Generic);
    }

    #[test]
    fn test_strip_ansi_basic_colors() {
        let input = b"\x1b[38;5;33m\xe2\x95\xad\xe2\x94\x80\x1b[0m";
        let result = strip_ansi(input);
        assert_eq!(result, "\u{256d}\u{2500}");
    }

    #[test]
    fn test_strip_ansi_no_escapes() {
        let input = b"hello world";
        assert_eq!(strip_ansi(input), "hello world");
    }

    #[test]
    fn test_strip_ansi_mixed_content() {
        let input = b"\x1b[1mBold\x1b[0m normal \x1b[31mred\x1b[0m";
        assert_eq!(strip_ansi(input), "Bold normal red");
    }

    #[test]
    fn test_analyze_output_completion_claude() {
        let output = "Some work output...\n\u{256d}\u{2500} ".as_bytes();
        let signal = analyze_output(output, &AgentType::ClaudeCode, 4096);
        assert_eq!(signal, AgentSignal::Completion);
    }

    #[test]
    fn test_analyze_output_error_claude() {
        let output = b"thread 'main' panicked at 'index out of bounds'";
        let signal = analyze_output(output, &AgentType::ClaudeCode, 4096);
        assert!(matches!(signal, AgentSignal::Error(_)));
    }

    #[test]
    fn test_analyze_output_idle() {
        let output = b"Working on something...";
        let signal = analyze_output(output, &AgentType::ClaudeCode, 4096);
        assert_eq!(signal, AgentSignal::Idle);
    }

    #[test]
    fn test_analyze_output_with_ansi() {
        let output = b"\x1b[31mError: something broke\x1b[0m";
        let signal = analyze_output(output, &AgentType::Generic, 4096);
        assert!(matches!(signal, AgentSignal::Error(_)));
    }

    #[test]
    fn test_analyze_output_sliding_window() {
        let mut output = vec![0u8; 8192];
        output[0..6].copy_from_slice(b"Error:");
        let signal = analyze_output(&output, &AgentType::ClaudeCode, 4096);
        // Error is outside the 4KB window
        assert_eq!(signal, AgentSignal::Idle);
    }

    #[test]
    fn test_build_command_claude() {
        let cmd = build_command("claude-code", "/path", false, false, None);
        assert!(cmd.contains("claude"));
        assert!(!cmd.contains("--session-id"));
        assert!(!cmd.contains("fix")); // task is no longer included in command
    }

    #[test]
    fn test_build_command_claude_skip_permissions() {
        let cmd = build_command("claude-code", "/path", true, false, None);
        assert!(cmd.contains("--dangerously-skip-permissions"));
    }

    #[test]
    fn test_build_command_claude_with_session_uuid() {
        let cmd = build_command("claude-code", "/path", true, false, Some("abc-123"));
        assert!(cmd.contains("--session-id abc-123"));
        assert!(cmd.ends_with("\n"));
    }

    #[test]
    fn test_build_command_other_agents_ignore_session_uuid() {
        let cmd = build_command("aider", "/path", false, false, Some("abc-123"));
        assert_eq!(cmd, "aider\n");
    }

    #[test]
    fn test_build_resume_command() {
        let resume =
            build_resume_command("claude-code", Some("11111111-2222-3333-4444-555555555555"), false);
        assert!(resume.contains("claude --resume 11111111-2222-3333-4444-555555555555"));
        // Legacy rows (no recorded session id) keep the --continue fallback.
        let legacy = build_resume_command("claude-code", None, false);
        assert!(legacy.contains("claude --continue"));
        assert!(!legacy.contains("--resume"));
        // Agents with no resume concept are simply relaunched.
        assert_eq!(build_resume_command("aider", None, false), "aider\n");
    }

    #[test]
    fn test_build_resume_command_rejects_non_uuid() {
        // A non-UUID id means a tampered DB — never interpolate it into the
        // shell command; fall back to --continue.
        let cmd = build_resume_command("claude-code", Some("x; rm -rf ~"), false);
        assert!(!cmd.contains("rm -rf"));
        assert!(cmd.contains("claude --continue"));
    }

    #[test]
    fn test_new_agent_session_id() {
        let id = new_agent_session_id("claude-code").expect("claude gets a session id");
        assert_eq!(id.len(), 36); // uuid v4 shape
        assert!(new_agent_session_id("aider").is_none());
        assert!(new_agent_session_id("codex").is_none());
    }

    #[test]
    fn test_is_resume_failure() {
        assert!(is_resume_failure(
            "No conversation found with session ID: 11111111-2222-3333-4444-555555555555"
        ));
        assert!(is_resume_failure("No conversation found to continue"));
        assert!(!is_resume_failure("Resumed. ? for shortcuts"));
    }

    #[test]
    fn test_inject_task_aider() {
        let input = inject_task_input(&AgentType::Aider, "fix the bug");
        assert_eq!(String::from_utf8(input).unwrap(), "/ask fix the bug\r");
    }

    #[test]
    fn test_trust_dialog_detected() {
        let trust = "Quick safety check: Is this a project you created or one you trust?\n  1. Yes, I trust this folder\n  2. No, exit";
        assert!(is_trust_dialog(trust));
        // The real prompt must NOT look like the trust dialog.
        let ready = "❯ \n  ⏵⏵ bypass permissions on (shift+tab to cycle)";
        assert!(!is_trust_dialog(ready));
    }

    #[test]
    fn test_claude_ready_not_triggered_by_trust_dialog() {
        // The trust dialog renders a `❯`, but is NOT "ready" — injecting there
        // would drop the task into the menu.
        let trust = "❯ 1. Yes, I trust this folder\n  2. No, exit\nEnter to confirm";
        assert!(!is_agent_ready(&AgentType::ClaudeCode, trust));

        // The real input prompt is "ready".
        let ready_bypass = "❯ \n  ⏵⏵ bypass permissions on (shift+tab to cycle)";
        assert!(is_agent_ready(&AgentType::ClaudeCode, ready_bypass));
        let ready_hint = "╭─────╮\n│ >    │\n╰─────╯\n  ? for shortcuts";
        assert!(is_agent_ready(&AgentType::ClaudeCode, ready_hint));
    }

    #[test]
    fn test_generic_ready_still_uses_prompt_chars() {
        assert!(is_agent_ready(&AgentType::Generic, "user@host:~$ "));
        assert!(!is_agent_ready(&AgentType::Generic, "still working..."));
    }
}
