mod commands;
mod events;
pub mod ssh;
mod transport;
mod ws_server;

use racc_core::events::EventBus;
use rusqlite::Connection;
use std::sync::{Arc, Mutex};
use tauri::{Emitter, Manager};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let db = commands::db::init_db().expect("Failed to initialize database");
    let db_arc: Arc<Mutex<Connection>> = Arc::new(Mutex::new(db));
    let (event_tx, _event_rx) = events::create_event_bus();
    let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);

    // Create racc-core components
    let transport_manager = racc_core::transport::manager::TransportManager::new();
    let ssh_manager = Arc::new(racc_core::ssh::SshManager::new());
    let event_bus: Arc<racc_core::events::BroadcastEventBus> =
        Arc::new(racc_core::events::BroadcastEventBus::new());
    let (terminal_tx, _terminal_rx) = tokio::sync::broadcast::channel::<racc_core::TerminalData>(256);

    // Build AppContext for racc-core commands
    let app_context = racc_core::AppContext::new(
        db_arc.clone(),
        transport_manager,
        ssh_manager,
        event_bus.clone(),
        terminal_tx.clone(),
    );

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        // AppContext is the main state container for racc-core commands
        .manage(app_context)
        // Keep Arc<Mutex<Connection>> separately for assistant.rs (unchanged)
        .manage(db_arc.clone())
        // Keep SidecarState for assistant.rs
        .manage(tokio::sync::Mutex::new(
            commands::assistant::SidecarState::new(),
        ))
        // Keep EventSender for ws_server and assistant event emission
        .manage(event_tx)
        .setup(move |app| {
            // Start buffer task — must use tauri::async_runtime::spawn because
            // setup() runs on the main thread outside the Tokio runtime context
            let ctx: tauri::State<racc_core::AppContext> = app.state();
            tauri::async_runtime::spawn(ctx.transport_manager.buffer_task());

            // Forwarder: terminal_tx -> app.emit("transport:data")
            let app_handle_terminal = app.handle().clone();
            let mut terminal_rx = terminal_tx.subscribe();
            tauri::async_runtime::spawn(async move {
                while let Ok(data) = terminal_rx.recv().await {
                    let _ = app_handle_terminal.emit(
                        "transport:data",
                        serde_json::json!({
                            "session_id": data.session_id,
                            "data": data.data,
                        }),
                    );
                }
            });

            // Forwarder: event_bus -> app.emit("racc://event")
            let app_handle_events = app.handle().clone();
            let mut event_rx = event_bus.subscribe();
            tauri::async_runtime::spawn(async move {
                while let Ok(event) = event_rx.recv().await {
                    let _ = app_handle_events.emit("racc://event", &event);
                }
            });

            use tauri::menu::{MenuBuilder, SubmenuBuilder};

            let reset_db_item = tauri::menu::MenuItemBuilder::new("Reset Database...")
                .id("reset_db")
                .build(app)?;

            let app_menu = SubmenuBuilder::new(app, "Racc")
                .hide()
                .hide_others()
                .show_all()
                .separator()
                .item(&reset_db_item)
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

            app.on_menu_event(|app_handle, event| {
                if event.id().as_ref() == "reset_db" {
                    let _ = app_handle.emit("menu-reset-db", ());
                }
            });

            let app_handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                ws_server::start(app_handle, shutdown_rx).await;
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
            commands::task::update_task_images,
            commands::task::delete_task,
            commands::task::save_task_image,
            commands::task::copy_file_to_task_images,
            commands::task::delete_task_image,
            commands::task::rename_task_image,
            commands::db::reset_db,
            commands::shell::open_url,
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
            commands::transport::transport_is_alive,
            commands::server::add_server,
            commands::server::update_server,
            commands::server::remove_server,
            commands::server::list_servers,
            commands::server::connect_server,
            commands::server::disconnect_server,
            commands::server::test_connection,
            commands::server::execute_remote_command,
            commands::server::list_ssh_config_hosts,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
