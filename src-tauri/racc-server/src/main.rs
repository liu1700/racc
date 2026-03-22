mod http;
mod ws;

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use axum::{routing::get, Router};
use racc_core::events::BroadcastEventBus;
use racc_core::ssh::SshManager;
use racc_core::transport::manager::TransportManager;
use tokio::net::TcpListener;

#[tokio::main]
async fn main() {
    env_logger::init();

    let db_path = std::env::var("RACC_DB_PATH")
        .map(PathBuf::from)
        .unwrap_or_else(|_| dirs::home_dir().unwrap().join(".racc/racc.db"));

    let dist_path = std::env::var("RACC_DIST_PATH")
        .unwrap_or_else(|_| "dist".to_string());

    let event_bus = Arc::new(BroadcastEventBus::new());

    // Initialize database
    let conn = racc_core::db::init_db(db_path)
        .expect("Failed to initialize database");
    let db = Arc::new(std::sync::Mutex::new(conn));

    // Initialize transport manager
    let transport_manager = TransportManager::new();

    // Initialize SSH manager
    let ssh_manager = Arc::new(SshManager::new());

    // Create terminal broadcast channel
    let (terminal_tx, _terminal_rx) = tokio::sync::broadcast::channel(256);

    let ctx = racc_core::AppContext::new(
        db,
        transport_manager,
        ssh_manager,
        event_bus,
        terminal_tx,
    );

    // Reconcile stale sessions
    if let Err(e) = racc_core::commands::session::reconcile_sessions(&ctx).await {
        eprintln!("Warning: session reconciliation failed: {}", e);
    }

    tokio::spawn(ctx.transport_manager.buffer_task());

    let ctx = Arc::new(ctx);

    // Start the supervisor reconciliation loop
    let supervisor_interval: u64 = std::env::var("RACC_SUPERVISOR_INTERVAL_MS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(5000);
    let supervisor = racc_core::supervisor::Supervisor::new(
        Arc::clone(&ctx),
        supervisor_interval,
    );
    let _supervisor_handle = supervisor.start();

    let app = Router::new()
        .route("/ws", get(ws::ws_handler))
        .fallback_service(http::static_file_service(&dist_path))
        .with_state(ctx);

    let port: u16 = std::env::var("RACC_PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(9399);
    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    let listener = TcpListener::bind(addr).await.unwrap();
    println!("racc-server listening on http://{}", addr);

    let shutdown = async {
        tokio::signal::ctrl_c().await.ok();
        println!("\nShutting down...");
    };
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown)
        .await
        .unwrap();
}
