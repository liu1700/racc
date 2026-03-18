use std::fs;
use std::path::PathBuf;
use tauri::State;

fn db_path() -> Result<PathBuf, String> {
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or("Could not find home directory")?;
    let dir = home.join(".racc");
    fs::create_dir_all(&dir).map_err(|e| format!("Failed to create ~/.racc: {e}"))?;
    Ok(dir.join("racc.db"))
}

pub fn init_db() -> Result<rusqlite::Connection, String> {
    let path = db_path()?;
    racc_core::db::init_db(path).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn reset_db(ctx: State<'_, racc_core::AppContext>) -> Result<(), String> {
    let path = db_path()?;

    // Lock the DB
    let mut conn = ctx.db.lock().map_err(|e| format!("Failed to lock db: {e}"))?;

    // Remove the database file
    if path.exists() {
        fs::remove_file(&path).map_err(|e| format!("Failed to delete database: {e}"))?;
    }

    // Reinitialize
    let new_conn = racc_core::db::init_db(path).map_err(|e| e.to_string())?;
    *conn = new_conn;

    Ok(())
}
