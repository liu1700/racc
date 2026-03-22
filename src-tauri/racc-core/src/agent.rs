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

/// Build the shell command to launch an agent (moved from session.rs).
/// Returns only the launch command — task description is sent separately
/// via PTY write after the agent initializes, avoiding all shell escaping issues.
pub fn build_command(agent: &str, _cwd: &str, skip_permissions: bool, rtk_remote: bool) -> String {
    match agent {
        "claude-code" => {
            let dangerously = if skip_permissions { " --dangerously-skip-permissions" } else { "" };
            let prefix = if rtk_remote { "PATH=$HOME/.racc/bin:$PATH " } else { "" };
            format!("{}claude{}\n", prefix, dangerously)
        }
        "aider" => "aider\n".to_string(),
        "codex" => "codex\n".to_string(),
        _ => format!("{}\n", agent),
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
        let cmd = build_command("claude-code", "/path", false, false);
        assert!(cmd.contains("claude"));
        assert!(!cmd.contains("fix")); // task is no longer included in command
    }

    #[test]
    fn test_build_command_claude_skip_permissions() {
        let cmd = build_command("claude-code", "/path", true, false);
        assert!(cmd.contains("--dangerously-skip-permissions"));
    }

    #[test]
    fn test_inject_task_aider() {
        let input = inject_task_input(&AgentType::Aider, "fix the bug");
        assert_eq!(String::from_utf8(input).unwrap(), "/ask fix the bug\r");
    }
}
