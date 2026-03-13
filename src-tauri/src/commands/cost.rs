use serde::{Deserialize, Serialize};
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::time::{Duration, SystemTime};

// --- JSONL deserialization types ---

#[derive(Deserialize)]
struct JsonlLine {
    message: Option<MessagePayload>,
}

#[derive(Deserialize)]
struct MessagePayload {
    usage: Option<UsageFields>,
}

#[derive(Deserialize)]
struct UsageFields {
    input_tokens: Option<u64>,
    output_tokens: Option<u64>,
    cache_creation_input_tokens: Option<u64>,
    cache_read_input_tokens: Option<u64>,
}

// --- Return types ---

#[derive(Debug, Clone, Serialize)]
pub struct SessionCost {
    pub session_id: String,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_creation_tokens: u64,
    pub cache_read_tokens: u64,
    /// Seconds since UNIX epoch when the JSONL file was last modified
    pub modified_at: u64,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct ProjectCosts {
    pub sessions: Vec<SessionCost>,
    pub total_input_tokens: u64,
    pub total_output_tokens: u64,
    pub total_cache_creation_tokens: u64,
    pub total_cache_read_tokens: u64,
    pub week_input_tokens: u64,
    pub week_output_tokens: u64,
}

// --- Core logic ---

fn parse_jsonl_file(path: &std::path::Path) -> SessionCost {
    let session_id = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("unknown")
        .to_string();

    let mut input_tokens: u64 = 0;
    let mut output_tokens: u64 = 0;
    let mut cache_creation_tokens: u64 = 0;
    let mut cache_read_tokens: u64 = 0;

    let file = match fs::File::open(path) {
        Ok(f) => f,
        Err(_) => {
            return SessionCost {
                session_id,
                input_tokens: 0,
                output_tokens: 0,
                cache_creation_tokens: 0,
                cache_read_tokens: 0,
                modified_at: 0,
            };
        }
    };

    let modified_at = file
        .metadata()
        .ok()
        .and_then(|m| m.modified().ok())
        .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .unwrap_or(0);

    let reader = BufReader::new(file);
    for line in reader.lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => continue,
        };

        if line.is_empty() {
            continue;
        }

        let parsed: JsonlLine = match serde_json::from_str(&line) {
            Ok(p) => p,
            Err(_) => continue,
        };

        let message = match parsed.message {
            Some(m) => m,
            None => continue,
        };

        let usage = match message.usage {
            Some(u) => u,
            None => continue,
        };

        input_tokens += usage.input_tokens.unwrap_or(0);
        output_tokens += usage.output_tokens.unwrap_or(0);
        cache_creation_tokens += usage.cache_creation_input_tokens.unwrap_or(0);
        cache_read_tokens += usage.cache_read_input_tokens.unwrap_or(0);
    }

    SessionCost {
        session_id,
        input_tokens,
        output_tokens,
        cache_creation_tokens,
        cache_read_tokens,
        modified_at,
    }
}

fn encode_path(path: &str) -> String {
    path.replace('/', "-")
}

fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}

// --- Tauri command ---

#[tauri::command]
pub async fn get_project_costs(worktree_path: String) -> Result<ProjectCosts, String> {
    let home = home_dir().ok_or("Could not find home directory")?;
    let encoded = encode_path(&worktree_path);
    let project_dir = home.join(".claude").join("projects").join(&encoded);

    if !project_dir.exists() {
        return Ok(ProjectCosts::default());
    }

    let mut sessions: Vec<SessionCost> = Vec::new();

    if let Ok(entries) = fs::read_dir(&project_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("jsonl") {
                sessions.push(parse_jsonl_file(&path));
            }
        }
    }

    let total_input: u64 = sessions.iter().map(|s| s.input_tokens).sum();
    let total_output: u64 = sessions.iter().map(|s| s.output_tokens).sum();
    let total_cache_creation: u64 = sessions.iter().map(|s| s.cache_creation_tokens).sum();
    let total_cache_read: u64 = sessions.iter().map(|s| s.cache_read_tokens).sum();

    // Sessions modified within the last 7 days
    let week_ago = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or(Duration::ZERO)
        .as_secs()
        .saturating_sub(7 * 24 * 3600);
    let week_input: u64 = sessions
        .iter()
        .filter(|s| s.modified_at >= week_ago)
        .map(|s| s.input_tokens)
        .sum();
    let week_output: u64 = sessions
        .iter()
        .filter(|s| s.modified_at >= week_ago)
        .map(|s| s.output_tokens)
        .sum();

    Ok(ProjectCosts {
        sessions,
        total_input_tokens: total_input,
        total_output_tokens: total_output,
        total_cache_creation_tokens: total_cache_creation,
        total_cache_read_tokens: total_cache_read,
        week_input_tokens: week_input,
        week_output_tokens: week_output,
    })
}
