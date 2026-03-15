mod commands;
mod events;
mod transport;
mod ws_server;

use rusqlite::Connection;
use std::sync::{Arc, Mutex};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let db = commands::db::init_db().expect("Failed to initialize database");
    let db_arc: Arc<Mutex<Connection>> = Arc::new(Mutex::new(db));
    let (event_tx, _event_rx) = events::create_event_bus();
    let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
    let transport_manager = crate::transport::manager::TransportManager::new();
    transport_manager.start_buffer_task();

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .manage(db_arc.clone())
        .manage(tokio::sync::Mutex::new(commands::assistant::SidecarState::new()))
        .manage(event_tx)
        .manage(transport_manager)
        .setup(move |app| {
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

            let app_handle = app.handle().clone();
            let db_for_ws = db_arc.clone();
            tauri::async_runtime::spawn(async move {
                ws_server::start(app_handle, db_for_ws, shutdown_rx).await;
            });

            Ok(())
        })
        .on_window_event(move |_window, event| {
            if let tauri::WindowEvent::Destroyed = event {
                let _ = shutdown_tx.send(true);
            }
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
            commands::transport::transport_write,
            commands::transport::transport_resize,
            commands::transport::transport_get_buffer,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
