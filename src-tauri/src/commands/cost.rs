use serde::Serialize;
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Serialize)]
pub struct UsageData {
    pub total_tokens: u64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub estimated_cost_usd: f64,
}

#[tauri::command]
pub async fn get_usage() -> Result<UsageData, String> {
    // Claude Code stores usage data in ~/.claude/usage/
    let home = dirs_next().ok_or("Could not find home directory")?;
    let usage_dir = home.join(".claude").join("usage");

    if !usage_dir.exists() {
        return Ok(UsageData {
            total_tokens: 0,
            input_tokens: 0,
            output_tokens: 0,
            estimated_cost_usd: 0.0,
        });
    }

    // Read the latest usage file
    // This is a simplified implementation — will need refinement based on
    // actual Claude Code usage data format
    let mut total_input: u64 = 0;
    let mut total_output: u64 = 0;

    if let Ok(entries) = fs::read_dir(&usage_dir) {
        for entry in entries.flatten() {
            if let Ok(content) = fs::read_to_string(entry.path()) {
                if let Ok(data) = serde_json::from_str::<serde_json::Value>(&content) {
                    total_input += data["input_tokens"].as_u64().unwrap_or(0);
                    total_output += data["output_tokens"].as_u64().unwrap_or(0);
                }
            }
        }
    }

    let total = total_input + total_output;
    // Rough cost estimate (Claude 3.5 Sonnet pricing as baseline)
    let cost = (total_input as f64 * 3.0 / 1_000_000.0) + (total_output as f64 * 15.0 / 1_000_000.0);

    Ok(UsageData {
        total_tokens: total,
        input_tokens: total_input,
        output_tokens: total_output,
        estimated_cost_usd: cost,
    })
}

fn dirs_next() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}
