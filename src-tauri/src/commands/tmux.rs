use std::process::Command;

#[tauri::command]
pub async fn send_keys(session_id: String, keys: String) -> Result<(), String> {
    Command::new("tmux")
        .args(["send-keys", "-t", &session_id, &keys, "Enter"])
        .output()
        .map_err(|e| format!("Failed to send keys: {}", e))?;

    Ok(())
}

#[tauri::command]
pub async fn capture_pane(session_id: String) -> Result<String, String> {
    let output = Command::new("tmux")
        .args(["capture-pane", "-t", &session_id, "-p", "-S", "-1000"])
        .output()
        .map_err(|e| format!("Failed to capture pane: {}", e))?;

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}
