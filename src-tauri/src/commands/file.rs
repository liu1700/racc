use ignore::WalkBuilder;
use nucleo_matcher::pattern::{Atom, AtomKind, CaseMatching, Normalization};
use nucleo_matcher::{Config, Matcher, Utf32Str};
use rusqlite::Connection;
use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

const MAX_LINES_DEFAULT: usize = 10_000;

#[derive(Debug, Serialize)]
pub struct FileContent {
    pub content: String,
    pub line_count: usize,
    pub total_lines: usize,
    pub language: String,
    pub encoding: String,
    pub file_path: String,
    pub is_truncated: bool,
}

#[derive(Debug, Serialize)]
pub struct FileMatch {
    pub relative_path: String,
    pub score: u16,
}

/// Detect language from file extension
fn detect_language(path: &Path) -> String {
    match path.extension().and_then(|e| e.to_str()) {
        Some("rs") => "rust",
        Some("ts") | Some("tsx") => "typescript",
        Some("js") | Some("jsx") => "javascript",
        Some("py") => "python",
        Some("toml") => "toml",
        Some("json") => "json",
        Some("yaml") | Some("yml") => "yaml",
        Some("md") => "markdown",
        Some("html") => "html",
        Some("css") => "css",
        Some("sql") => "sql",
        Some("sh") | Some("bash") | Some("zsh") => "shellscript",
        Some("go") => "go",
        Some("java") => "java",
        Some("c") | Some("h") => "c",
        Some("cpp") | Some("hpp") | Some("cc") => "cpp",
        Some("rb") => "ruby",
        Some("swift") => "swift",
        Some("kt") => "kotlin",
        Some("lua") => "lua",
        Some("zig") => "zig",
        Some(ext) => ext,
        None => "plaintext",
    }
    .to_string()
}

/// Check if file content appears to be binary
fn is_binary(bytes: &[u8]) -> bool {
    let check_len = bytes.len().min(8192);
    bytes[..check_len].contains(&0)
}

/// Resolve the base directory for a session or repo.
fn resolve_base_path(
    conn: &Connection,
    session_id: Option<i64>,
    repo_id: Option<i64>,
) -> Result<PathBuf, String> {
    if let Some(sid) = session_id {
        let result: Result<(Option<String>, String), _> = conn.query_row(
            "SELECT s.worktree_path, r.path FROM sessions s JOIN repos r ON s.repo_id = r.id WHERE s.id = ?1",
            [sid],
            |row| Ok((row.get(0)?, row.get(1)?)),
        );
        match result {
            Ok((Some(wt), _)) => Ok(PathBuf::from(wt)),
            Ok((None, repo_path)) => Ok(PathBuf::from(repo_path)),
            Err(e) => Err(format!("Session not found: {}", e)),
        }
    } else if let Some(rid) = repo_id {
        let path: String = conn
            .query_row("SELECT path FROM repos WHERE id = ?1", [rid], |row| {
                row.get(0)
            })
            .map_err(|e| format!("Repo not found: {}", e))?;
        Ok(PathBuf::from(path))
    } else {
        Err("Either session_id or repo_id must be provided".to_string())
    }
}

/// Validate that a file path is within the allowed base directory (prevent path traversal)
fn validate_path(base: &Path, relative: &str) -> Result<PathBuf, String> {
    let full = base.join(relative);
    let canonical = full
        .canonicalize()
        .map_err(|e| format!("File not found: {}", e))?;
    let base_canonical = base
        .canonicalize()
        .map_err(|e| format!("Base path invalid: {}", e))?;

    if !canonical.starts_with(&base_canonical) {
        return Err("Access denied: path is outside the allowed directory".to_string());
    }
    Ok(canonical)
}

/// Core logic shared by the Tauri command and the assistant relay.
pub fn read_file_core(
    conn: &Connection,
    session_id: Option<i64>,
    repo_id: Option<i64>,
    file_path: &str,
    max_lines: Option<usize>,
) -> Result<FileContent, String> {
    let base = resolve_base_path(conn, session_id, repo_id)?;
    let full_path = validate_path(&base, file_path)?;

    let bytes = fs::read(&full_path).map_err(|e| format!("Cannot read file: {}", e))?;

    if is_binary(&bytes) {
        return Err("Binary file — cannot display".to_string());
    }

    let text = String::from_utf8(bytes)
        .map_err(|_| "File encoding not supported (not UTF-8)".to_string())?;
    let all_lines: Vec<&str> = text.lines().collect();
    let total_lines = all_lines.len();
    let limit = max_lines.unwrap_or(MAX_LINES_DEFAULT);
    let is_truncated = total_lines > limit;
    let content = if is_truncated {
        all_lines[..limit].join("\n")
    } else {
        text
    };
    let line_count = if is_truncated { limit } else { total_lines };
    let language = detect_language(&full_path);

    Ok(FileContent {
        content,
        line_count,
        total_lines,
        language,
        encoding: "utf-8".to_string(),
        file_path: file_path.to_string(),
        is_truncated,
    })
}

#[tauri::command]
pub async fn read_file(
    db: tauri::State<'_, Arc<Mutex<Connection>>>,
    session_id: Option<i64>,
    repo_id: Option<i64>,
    file_path: String,
    max_lines: Option<usize>,
) -> Result<FileContent, String> {
    let conn = db.lock().map_err(|e| e.to_string())?;
    read_file_core(&conn, session_id, repo_id, &file_path, max_lines)
}

#[tauri::command]
pub async fn search_files(
    db: tauri::State<'_, Arc<Mutex<Connection>>>,
    session_id: Option<i64>,
    repo_id: Option<i64>,
    query: String,
) -> Result<Vec<FileMatch>, String> {
    let base = {
        let conn = db.lock().map_err(|e| e.to_string())?;
        resolve_base_path(&conn, session_id, repo_id)?
    }; // DB lock released before filesystem walk

    // Collect file paths respecting .gitignore
    let mut paths: Vec<String> = Vec::new();
    for entry in WalkBuilder::new(&base).hidden(true).build().flatten() {
        if entry.file_type().map_or(false, |ft| ft.is_file()) {
            if let Ok(rel) = entry.path().strip_prefix(&base) {
                if let Some(s) = rel.to_str() {
                    paths.push(s.to_string());
                }
            }
        }
    }

    if query.is_empty() {
        paths.sort();
        paths.truncate(20);
        return Ok(paths
            .into_iter()
            .map(|p| FileMatch {
                relative_path: p,
                score: 0,
            })
            .collect());
    }

    // Fuzzy match using nucleo
    let mut matcher = Matcher::new(Config::DEFAULT);
    let atom = Atom::new(
        &query,
        CaseMatching::Smart,
        Normalization::Smart,
        AtomKind::Fuzzy,
        false,
    );

    let mut scored: Vec<FileMatch> = paths
        .iter()
        .filter_map(|path| {
            let mut buf = Vec::new();
            let haystack = Utf32Str::new(path, &mut buf);
            atom.score(haystack, &mut matcher).map(|score| FileMatch {
                relative_path: path.clone(),
                score,
            })
        })
        .collect();

    scored.sort_by(|a, b| b.score.cmp(&a.score));
    scored.truncate(20);

    Ok(scored)
}
