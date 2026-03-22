use tauri::State;
pub use racc_core::commands::server::{Server, ServerConfig};

#[tauri::command]
pub fn add_server(
    config: ServerConfig,
    ctx: State<'_, std::sync::Arc<racc_core::AppContext>>,
) -> Result<Server, String> {
    racc_core::commands::server::add_server(&ctx, config).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn update_server(
    server_id: String,
    config: ServerConfig,
    ctx: State<'_, std::sync::Arc<racc_core::AppContext>>,
) -> Result<Server, String> {
    racc_core::commands::server::update_server(&ctx, server_id, config)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn remove_server(
    server_id: String,
    ctx: State<'_, std::sync::Arc<racc_core::AppContext>>,
) -> Result<(), String> {
    racc_core::commands::server::remove_server(&ctx, server_id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn list_servers(ctx: State<'_, std::sync::Arc<racc_core::AppContext>>) -> Result<Vec<Server>, String> {
    racc_core::commands::server::list_servers(&ctx).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn connect_server(
    server_id: String,
    ctx: State<'_, std::sync::Arc<racc_core::AppContext>>,
) -> Result<(), String> {
    racc_core::commands::server::connect_server(&ctx, server_id)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn disconnect_server(
    server_id: String,
    ctx: State<'_, std::sync::Arc<racc_core::AppContext>>,
) -> Result<(), String> {
    racc_core::commands::server::disconnect_server(&ctx, server_id)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn test_connection(
    server_id: String,
    ctx: State<'_, std::sync::Arc<racc_core::AppContext>>,
) -> Result<String, String> {
    racc_core::commands::server::test_connection(&ctx, server_id)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn execute_remote_command(
    server_id: String,
    command: String,
    ctx: State<'_, std::sync::Arc<racc_core::AppContext>>,
) -> Result<racc_core::ssh::CommandOutput, String> {
    racc_core::commands::server::execute_remote_command(&ctx, server_id, command)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn list_ssh_config_hosts() -> Result<Vec<racc_core::ssh::config_parser::SshHostConfig>, String> {
    racc_core::commands::server::list_ssh_config_hosts()
        .await
        .map_err(|e| e.to_string())
}
