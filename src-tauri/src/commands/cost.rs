use serde::{Deserialize, Serialize};
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::PathBuf;

// --- Pricing per 1M tokens ---

struct ModelPricing {
    input: f64,
    output: f64,
    cache_write: f64,
    cache_read: f64,
}

const OPUS_PRICING: ModelPricing = ModelPricing {
    input: 15.0,
    output: 75.0,
    cache_write: 18.75,
    cache_read: 1.50,
};

const SONNET_PRICING: ModelPricing = ModelPricing {
    input: 3.0,
    output: 15.0,
    cache_write: 3.75,
    cache_read: 0.30,
};

const HAIKU_PRICING: ModelPricing = ModelPricing {
    input: 0.80,
    output: 4.0,
    cache_write: 1.0,
    cache_read: 0.08,
};

fn pricing_for_model(model: &str) -> &'static ModelPricing {
    let lower = model.to_lowercase();
    if lower.contains("opus") {
        &OPUS_PRICING
    } else if lower.contains("haiku") {
        &HAIKU_PRICING
    } else {
        &SONNET_PRICING
    }
}

// --- JSONL deserialization types ---

#[derive(Deserialize)]
struct JsonlLine {
    message: Option<MessagePayload>,
}

#[derive(Deserialize)]
struct MessagePayload {
    model: Option<String>,
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
    pub estimated_cost_usd: f64,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct ProjectCosts {
    pub sessions: Vec<SessionCost>,
    pub total_input_tokens: u64,
    pub total_output_tokens: u64,
    pub total_cache_creation_tokens: u64,
    pub total_cache_read_tokens: u64,
    pub total_estimated_cost_usd: f64,
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
    let mut estimated_cost: f64 = 0.0;

    let file = match fs::File::open(path) {
        Ok(f) => f,
        Err(_) => {
            return SessionCost {
                session_id,
                input_tokens: 0,
                output_tokens: 0,
                cache_creation_tokens: 0,
                cache_read_tokens: 0,
                estimated_cost_usd: 0.0,
            };
        }
    };

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

        let model_name = message.model.as_deref().unwrap_or("sonnet");
        let pricing = pricing_for_model(model_name);

        let inp = usage.input_tokens.unwrap_or(0);
        let out = usage.output_tokens.unwrap_or(0);
        let cw = usage.cache_creation_input_tokens.unwrap_or(0);
        let cr = usage.cache_read_input_tokens.unwrap_or(0);

        input_tokens += inp;
        output_tokens += out;
        cache_creation_tokens += cw;
        cache_read_tokens += cr;

        estimated_cost += (inp as f64 * pricing.input / 1_000_000.0)
            + (out as f64 * pricing.output / 1_000_000.0)
            + (cw as f64 * pricing.cache_write / 1_000_000.0)
            + (cr as f64 * pricing.cache_read / 1_000_000.0);
    }

    SessionCost {
        session_id,
        input_tokens,
        output_tokens,
        cache_creation_tokens,
        cache_read_tokens,
        estimated_cost_usd: (estimated_cost * 100.0).round() / 100.0,
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
        return Ok(ProjectCosts {
            sessions: vec![],
            total_input_tokens: 0,
            total_output_tokens: 0,
            total_cache_creation_tokens: 0,
            total_cache_read_tokens: 0,
            total_estimated_cost_usd: 0.0,
        });
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
    let total_cost: f64 = sessions.iter().map(|s| s.estimated_cost_usd).sum();

    Ok(ProjectCosts {
        sessions,
        total_input_tokens: total_input,
        total_output_tokens: total_output,
        total_cache_creation_tokens: total_cache_creation,
        total_cache_read_tokens: total_cache_read,
        total_estimated_cost_usd: (total_cost * 100.0).round() / 100.0,
    })
}
