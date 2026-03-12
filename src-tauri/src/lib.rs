mod commands;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .invoke_handler(tauri::generate_handler![
            commands::session::create_session,
            commands::session::list_sessions,
            commands::session::stop_session,
            commands::tmux::send_keys,
            commands::tmux::send_special_key,
            commands::tmux::capture_pane,
            commands::tmux::resize_pane,
            commands::git::create_worktree,
            commands::git::delete_worktree,
            commands::git::get_diff,
            commands::cost::get_project_costs,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
