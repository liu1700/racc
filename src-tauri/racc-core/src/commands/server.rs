use rusqlite::params;
use serde::{Deserialize, Serialize};

use crate::AppContext;
use crate::error::CoreError;
use crate::ssh::CommandOutput;
use crate::ssh::config_parser::SshHostConfig;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Server {
    pub id: String,
    pub name: String,
    pub host: String,
    pub port: i32,
    pub username: String,
    pub auth_method: String, // "key" | "ssh_config" | "agent"
    pub key_path: Option<String>,
    pub ssh_config_host: Option<String>,
    pub setup_status: String,
    pub setup_details: Option<String>,
    pub ai_provider: Option<String>,
    pub ai_api_key: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Deserialize)]
pub struct ServerConfig {
    pub name: String,
    pub host: String,
    pub port: Option<i32>,
    pub username: String,
    pub auth_method: String,
    pub key_path: Option<String>,
    pub ssh_config_host: Option<String>,
    pub ai_provider: Option<String>,
    pub ai_api_key: Option<String>,
}

/// Helper to read a single server from DB by ID.
pub fn get_server_by_id(conn: &rusqlite::Connection, server_id: &str) -> Result<Server, CoreError> {
    conn.query_row(
        "SELECT id, name, host, port, username, auth_method, key_path, ssh_config_host, setup_status, setup_details, ai_provider, ai_api_key, created_at, updated_at FROM servers WHERE id=?1",
        params![server_id],
        |row| Ok(Server {
            id: row.get(0)?,
            name: row.get(1)?,
            host: row.get(2)?,
            port: row.get(3)?,
            username: row.get(4)?,
            auth_method: row.get(5)?,
            key_path: row.get(6)?,
            ssh_config_host: row.get(7)?,
            setup_status: row.get(8)?,
            setup_details: row.get(9)?,
            ai_provider: row.get(10)?,
            ai_api_key: row.get(11)?,
            created_at: row.get(12)?,
            updated_at: row.get(13)?,
        }),
    ).map_err(CoreError::from)
}

pub fn add_server(
    ctx: &AppContext,
    config: ServerConfig,
) -> Result<Server, CoreError> {
    let conn = ctx.db.lock().map_err(|e| CoreError::Other(e.to_string()))?;
    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();
    let port = config.port.unwrap_or(22);

    conn.execute(
        "INSERT INTO servers (id, name, host, port, username, auth_method, key_path, ssh_config_host, ai_provider, ai_api_key, setup_status, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, 'pending', ?11, ?11)",
        params![
            id,
            config.name,
            config.host,
            port,
            config.username,
            config.auth_method,
            config.key_path,
            config.ssh_config_host,
            config.ai_provider,
            config.ai_api_key,
            now
        ],
    )?;

    get_server_by_id(&conn, &id)
}

pub fn update_server(
    ctx: &AppContext,
    server_id: String,
    config: ServerConfig,
) -> Result<Server, CoreError> {
    let conn = ctx.db.lock().map_err(|e| CoreError::Other(e.to_string()))?;
    let now = chrono::Utc::now().to_rfc3339();
    let port = config.port.unwrap_or(22);

    conn.execute(
        "UPDATE servers SET name=?1, host=?2, port=?3, username=?4, auth_method=?5, key_path=?6, ssh_config_host=?7, ai_provider=?8, ai_api_key=?9, updated_at=?10 WHERE id=?11",
        params![
            config.name,
            config.host,
            port,
            config.username,
            config.auth_method,
            config.key_path,
            config.ssh_config_host,
            config.ai_provider,
            config.ai_api_key,
            now,
            server_id
        ],
    )?;

    get_server_by_id(&conn, &server_id)
}

pub fn remove_server(
    ctx: &AppContext,
    server_id: String,
) -> Result<(), CoreError> {
    let conn = ctx.db.lock().map_err(|e| CoreError::Other(e.to_string()))?;
    conn.execute("DELETE FROM servers WHERE id=?1", params![server_id])?;
    Ok(())
}

pub fn list_servers(ctx: &AppContext) -> Result<Vec<Server>, CoreError> {
    let conn = ctx.db.lock().map_err(|e| CoreError::Other(e.to_string()))?;
    let mut stmt = conn.prepare(
        "SELECT id, name, host, port, username, auth_method, key_path, ssh_config_host, setup_status, setup_details, ai_provider, ai_api_key, created_at, updated_at FROM servers ORDER BY created_at DESC",
    )?;

    let servers = stmt
        .query_map([], |row| {
            Ok(Server {
                id: row.get(0)?,
                name: row.get(1)?,
                host: row.get(2)?,
                port: row.get(3)?,
                username: row.get(4)?,
                auth_method: row.get(5)?,
                key_path: row.get(6)?,
                ssh_config_host: row.get(7)?,
                setup_status: row.get(8)?,
                setup_details: row.get(9)?,
                ai_provider: row.get(10)?,
                ai_api_key: row.get(11)?,
                created_at: row.get(12)?,
                updated_at: row.get(13)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;

    Ok(servers)
}

pub async fn connect_server(
    ctx: &AppContext,
    server_id: String,
) -> Result<(), CoreError> {
    let server = {
        let conn = ctx.db.lock().map_err(|e| CoreError::Other(e.to_string()))?;
        get_server_by_id(&conn, &server_id)?
    };
    ctx.ssh_manager
        .connect(
            &server_id,
            &server.host,
            server.port as u16,
            &server.username,
            &server.auth_method,
            server.key_path.as_deref(),
        )
        .await
        .map_err(|e| CoreError::Ssh(e))
}

pub async fn disconnect_server(
    ctx: &AppContext,
    server_id: String,
) -> Result<(), CoreError> {
    ctx.ssh_manager
        .disconnect(&server_id)
        .await
        .map_err(|e| CoreError::Ssh(e))
}

pub async fn test_connection(
    ctx: &AppContext,
    server_id: String,
) -> Result<String, CoreError> {
    let server = {
        let conn = ctx.db.lock().map_err(|e| CoreError::Other(e.to_string()))?;
        get_server_by_id(&conn, &server_id)?
    };
    ctx.ssh_manager
        .connect(
            &server_id,
            &server.host,
            server.port as u16,
            &server.username,
            &server.auth_method,
            server.key_path.as_deref(),
        )
        .await
        .map_err(|e| CoreError::Ssh(e))?;
    let result = ctx
        .ssh_manager
        .exec(&server_id, "echo ok")
        .await
        .map_err(|e| CoreError::Ssh(e))?;
    ctx.ssh_manager
        .disconnect(&server_id)
        .await
        .map_err(|e| CoreError::Ssh(e))?;
    Ok(result.stdout)
}

pub async fn execute_remote_command(
    ctx: &AppContext,
    server_id: String,
    command: String,
) -> Result<CommandOutput, CoreError> {
    ctx.ssh_manager
        .exec(&server_id, &command)
        .await
        .map_err(|e| CoreError::Ssh(e))
}

pub async fn list_ssh_config_hosts() -> Result<Vec<SshHostConfig>, CoreError> {
    crate::ssh::config_parser::list_ssh_hosts().map_err(|e| CoreError::Ssh(e))
}
