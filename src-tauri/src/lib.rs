mod commands;
mod events;

use std::sync::Mutex;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let db = commands::db::init_db().expect("Failed to initialize database");
    let (event_tx, _event_rx) = events::create_event_bus();

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_pty::init())
        .manage(Mutex::new(db))
        .manage(tokio::sync::Mutex::new(commands::assistant::SidecarState::new()))
        .manage(event_tx)
        .setup(|app| {
            use tauri::menu::{MenuBuilder, SubmenuBuilder};

            let app_menu = SubmenuBuilder::new(app, "Racc")
                .hide()
                .hide_others()
                .show_all()
                .separator()
                .quit()
                .build()?;

            let edit_menu = SubmenuBuilder::new(app, "Edit")
                .undo()
                .redo()
                .separator()
                .cut()
                .copy()
                .paste()
                .select_all()
                .build()?;

            let menu = MenuBuilder::new(app)
                .item(&app_menu)
                .item(&edit_menu)
                .build()?;

            app.set_menu(menu)?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::session::import_repo,
            commands::session::list_repos,
            commands::session::remove_repo,
            commands::session::create_session,
            commands::session::stop_session,
            commands::session::remove_session,
            commands::session::reattach_session,
            commands::session::reconcile_sessions,
            commands::session::update_session_pr_url,
            commands::git::create_worktree,
            commands::git::delete_worktree,
            commands::git::get_diff,
            commands::cost::get_project_costs,
            commands::cost::get_global_costs,
            commands::assistant::get_assistant_config,
            commands::assistant::set_assistant_config,
            commands::assistant::save_assistant_message,
            commands::assistant::get_assistant_messages,
            commands::assistant::get_all_sessions_for_assistant,
            commands::assistant::get_session_diff_for_assistant,
            commands::assistant::get_session_costs_for_assistant,
            commands::assistant::read_file_for_assistant,
            commands::assistant::assistant_send_message,
            commands::assistant::assistant_read_response,
            commands::assistant::assistant_shutdown,
            commands::file::read_file,
            commands::file::search_files,
            commands::task::create_task,
            commands::task::list_tasks,
            commands::task::update_task_status,
            commands::task::update_task_description,
            commands::task::delete_task,
            commands::db::reset_db,
            commands::insights::record_session_events,
            commands::insights::get_insights,
            commands::insights::update_insight_status,
            commands::insights::save_insight,
            commands::insights::get_session_events,
            commands::insights::append_to_file,
            commands::insights::run_batch_analysis,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
