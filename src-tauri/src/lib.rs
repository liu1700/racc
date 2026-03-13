mod commands;

use std::sync::Mutex;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let db = commands::db::init_db().expect("Failed to initialize database");

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_pty::init())
        .manage(Mutex::new(db))
        .manage(tokio::sync::Mutex::new(commands::assistant::SidecarState::new()))
        .invoke_handler(tauri::generate_handler![
            commands::session::import_repo,
            commands::session::list_repos,
            commands::session::remove_repo,
            commands::session::create_session,
            commands::session::stop_session,
            commands::session::remove_session,
            commands::session::reconcile_sessions,
            commands::git::create_worktree,
            commands::git::delete_worktree,
            commands::git::get_diff,
            commands::cost::get_project_costs,
            commands::assistant::get_assistant_config,
            commands::assistant::set_assistant_config,
            commands::assistant::save_assistant_message,
            commands::assistant::get_assistant_messages,
            commands::assistant::get_all_sessions_for_assistant,
            commands::assistant::get_session_diff_for_assistant,
            commands::assistant::get_session_costs_for_assistant,
            commands::assistant::assistant_send_message,
            commands::assistant::assistant_read_response,
            commands::assistant::assistant_shutdown,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
