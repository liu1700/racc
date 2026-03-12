use std::process::Command;

#[tauri::command]
pub async fn send_keys(session_id: String, keys: String) -> Result<(), String> {
    // Send raw keys without auto-appending Enter.
    // The frontend is responsible for sending Enter when needed.
    Command::new("tmux")
        .args(["send-keys", "-t", &session_id, "-l", &keys])
        .output()
        .map_err(|e| format!("Failed to send keys: {}", e))?;

    Ok(())
}

#[tauri::command]
pub async fn send_special_key(session_id: String, key: String) -> Result<(), String> {
    // Send special keys like Enter, C-c, Escape, etc.
    Command::new("tmux")
        .args(["send-keys", "-t", &session_id, &key])
        .output()
        .map_err(|e| format!("Failed to send special key: {}", e))?;

    Ok(())
}

#[tauri::command]
pub async fn capture_pane(session_id: String) -> Result<String, String> {
    let output = Command::new("tmux")
        .args([
            "capture-pane",
            "-t",
            &session_id,
            "-p",    // print to stdout
            "-e",    // include escape sequences (ANSI colors)
            "-S",
            "-1000", // scroll buffer: last 1000 lines
        ])
        .output()
        .map_err(|e| format!("Failed to capture pane: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("capture-pane failed: {}", stderr));
    }

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

#[tauri::command]
pub async fn resize_pane(session_id: String, cols: u32, rows: u32) -> Result<(), String> {
    Command::new("tmux")
        .args([
            "resize-window",
            "-t",
            &session_id,
            "-x",
            &cols.to_string(),
            "-y",
            &rows.to_string(),
        ])
        .output()
        .map_err(|e| format!("Failed to resize pane: {}", e))?;

    Ok(())
}
