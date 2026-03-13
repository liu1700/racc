# File Viewer Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a read-only file viewer to Racc with three trigger mechanisms (Cmd+P command palette, Pi Agent inline preview, terminal path click) and a full overlay viewer with syntax highlighting.

**Architecture:** New Rust commands (`read_file`, `search_files`) provide file access. A Zustand store (`fileViewerStore`) coordinates three trigger mechanisms that all feed into a single `FileViewer` overlay component rendered in `App.tsx`. Syntax highlighting via Shiki. Pi Agent gets a new `read_file` tool for inline code snippets with upgrade-to-overlay capability.

**Tech Stack:** Rust (`ignore` + `nucleo` crates), React 19, Zustand, Shiki, xterm.js link provider API, Tailwind CSS with existing design tokens.

---

## Chunk 1: Rust Backend Commands

### Task 1: Add Rust dependencies

**Files:**
- Modify: `src-tauri/Cargo.toml`

- [ ] **Step 1: Add `ignore` and `nucleo` crates to Cargo.toml**

Add after the `chrono` dependency (line ~22 of `src-tauri/Cargo.toml`):

```toml
ignore = "0.4"
nucleo-matcher = "0.3"
```

- [ ] **Step 2: Verify dependencies resolve**

Run: `cd src-tauri && cargo check`
Expected: Compiles successfully with new dependencies downloaded.

- [ ] **Step 3: Commit**

```bash
git add src-tauri/Cargo.toml src-tauri/Cargo.lock
git commit -m "chore: add ignore and nucleo-matcher crates for file search"
```

---

### Task 2: Create `read_file` Rust command

**Files:**
- Create: `src-tauri/src/commands/file.rs`
- Modify: `src-tauri/src/commands/mod.rs`

- [ ] **Step 1: Create `commands/file.rs` with types and `read_file` command**

```rust
use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};
use rusqlite::Connection;
use std::sync::Mutex;

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
/// If session has a worktree_path, use that. Otherwise use repo path.
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
            .query_row("SELECT path FROM repos WHERE id = ?1", [rid], |row| row.get(0))
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

#[tauri::command]
/// Core logic shared by the Tauri command and the assistant relay.
/// Does NOT take tauri::State — takes a pre-locked &Connection instead.
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

    let text = String::from_utf8(bytes).map_err(|_| "File encoding not supported (not UTF-8)".to_string())?;
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
    db: tauri::State<'_, Mutex<Connection>>,
    session_id: Option<i64>,
    repo_id: Option<i64>,
    file_path: String,
    max_lines: Option<usize>,
) -> Result<FileContent, String> {
    let conn = db.lock().map_err(|e| e.to_string())?;
    read_file_core(&conn, session_id, repo_id, &file_path, max_lines)
}
```

- [ ] **Step 2: Register module in `commands/mod.rs`**

Add to `src-tauri/src/commands/mod.rs`:

```rust
pub mod file;
```

- [ ] **Step 3: Verify compilation**

Run: `cd src-tauri && cargo check`
Expected: Compiles successfully.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/commands/file.rs src-tauri/src/commands/mod.rs
git commit -m "feat(file): add read_file command with path validation and binary detection"
```

---

### Task 3: Create `search_files` Rust command

**Files:**
- Modify: `src-tauri/src/commands/file.rs`

- [ ] **Step 1: Add `search_files` command to `file.rs`**

Append to `src-tauri/src/commands/file.rs`:

```rust
use ignore::WalkBuilder;
use nucleo_matcher::pattern::{Atom, AtomKind, CaseMatching, Normalization};
use nucleo_matcher::{Config, Matcher, Utf32Str};

#[derive(Debug, Serialize)]
pub struct FileMatch {
    pub relative_path: String,
    pub score: u16,
}

#[tauri::command]
pub async fn search_files(
    db: tauri::State<'_, Mutex<Connection>>,
    session_id: Option<i64>,
    repo_id: Option<i64>,
    query: String,
) -> Result<Vec<FileMatch>, String> {
    let conn = db.lock().map_err(|e| e.to_string())?;
    let base = resolve_base_path(&conn, session_id, repo_id)?;

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
        // Return first 20 files sorted alphabetically when no query
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
```

- [ ] **Step 2: Verify compilation**

Run: `cd src-tauri && cargo check`
Expected: Compiles successfully.

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/commands/file.rs
git commit -m "feat(file): add search_files command with fuzzy matching"
```

---

### Task 4: Register commands and add assistant relay

**Files:**
- Modify: `src-tauri/src/lib.rs`
- Modify: `src-tauri/src/commands/assistant.rs`

- [ ] **Step 1: Register `read_file` and `search_files` in `lib.rs`**

Add to the `generate_handler!` macro list in `src-tauri/src/lib.rs` (after the existing assistant commands):

```rust
commands::file::read_file,
commands::file::search_files,
```

- [ ] **Step 2: Add `read_file_for_assistant` command to `assistant.rs`**

Add a relay command near the other `_for_assistant` commands (after `get_session_costs_for_assistant`):

```rust
#[tauri::command]
pub async fn read_file_for_assistant(
    db: tauri::State<'_, Mutex<Connection>>,
    session_id: Option<i64>,
    repo_id: Option<i64>,
    file_path: String,
) -> Result<String, String> {
    let conn = db.lock().map_err(|e| e.to_string())?;
    let result = crate::commands::file::read_file_core(
        &conn, session_id, repo_id, &file_path, Some(200),
    )?;
    serde_json::to_string(&result).map_err(|e| e.to_string())
}
```

- [ ] **Step 3: Register `read_file_for_assistant` in `lib.rs`**

Add to the `generate_handler!` macro list:

```rust
commands::assistant::read_file_for_assistant,
```

- [ ] **Step 4: Add `read_file` to assistant tool call handler**

In `assistant.rs`, find the tool call match block in `assistant_read_response` (the section that matches tool names like `get_all_sessions`, `get_session_diff`, `get_session_costs`). Add a new arm:

```rust
"read_file" => {
    let file_path = args.get("file_path")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let session_id = args.get("session_id")
        .and_then(|v| v.as_i64());
    let repo_id = args.get("repo_id")
        .and_then(|v| v.as_i64());

    let conn = db.lock().map_err(|e| e.to_string())?;
    match crate::commands::file::read_file_core(
        &conn, session_id, repo_id, &file_path, Some(200),
    ) {
        Ok(content) => serde_json::to_string(&content).unwrap_or_default(),
        Err(e) => format!("Error reading file: {}", e),
    }
}
```

- [ ] **Step 5: Verify compilation**

Run: `cd src-tauri && cargo check`
Expected: Compiles successfully.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/lib.rs src-tauri/src/commands/assistant.rs
git commit -m "feat(file): register file commands and add assistant relay"
```

---

## Chunk 2: Frontend Foundation

### Task 5: Add Shiki dependency

**Files:**
- Modify: `package.json`

- [ ] **Step 1: Install Shiki**

Run: `cd /home/devuser/racc && bun add shiki`

- [ ] **Step 2: Verify install**

Run: `bun run build`
Expected: Build succeeds.

- [ ] **Step 3: Commit**

```bash
git add package.json bun.lockb
git commit -m "chore: add shiki for syntax highlighting"
```

---

### Task 6: Create TypeScript types for file viewer

**Files:**
- Create: `src/types/file.ts`

- [ ] **Step 1: Create type definitions**

Create `src/types/file.ts`:

```typescript
export interface FileContent {
  content: string;
  line_count: number;
  total_lines: number;
  language: string;
  encoding: string;
  file_path: string;
  is_truncated: boolean;
}

export interface FileMatch {
  relative_path: string;
  score: number;
}
```

- [ ] **Step 2: Commit**

```bash
git add src/types/file.ts
git commit -m "feat(file): add file viewer TypeScript types"
```

---

### Task 7: Create file viewer Zustand store

**Files:**
- Create: `src/stores/fileViewerStore.ts`

- [ ] **Step 1: Create the store**

Create `src/stores/fileViewerStore.ts`:

```typescript
import { create } from "zustand";
import { invoke } from "@tauri-apps/api/core";
import type { FileContent, FileMatch } from "../types/file";

interface FileViewerState {
  // Overlay state
  isOpen: boolean;
  filePath: string | null;
  content: FileContent | null;
  loading: boolean;
  error: string | null;
  scrollToLine: number | null;
  highlightRange: [number, number] | null;

  // Command palette state
  isPaletteOpen: boolean;
  searchQuery: string;
  searchResults: FileMatch[];
  searchLoading: boolean;

  // Actions
  openFile: (params: {
    sessionId?: number | null;
    repoId?: number | null;
    filePath: string;
    scrollToLine?: number;
    highlightRange?: [number, number];
  }) => Promise<void>;
  closeViewer: () => void;
  openPalette: () => void;
  closePalette: () => void;
  searchFiles: (params: {
    sessionId?: number | null;
    repoId?: number | null;
    query: string;
  }) => Promise<void>;
}

export const useFileViewerStore = create<FileViewerState>((set, get) => ({
  isOpen: false,
  filePath: null,
  content: null,
  loading: false,
  error: null,
  scrollToLine: null,
  highlightRange: null,

  isPaletteOpen: false,
  searchQuery: "",
  searchResults: [],
  searchLoading: false,

  openFile: async ({ sessionId, repoId, filePath, scrollToLine, highlightRange }) => {
    set({
      isOpen: true,
      loading: true,
      error: null,
      filePath,
      scrollToLine: scrollToLine ?? null,
      highlightRange: highlightRange ?? null,
      isPaletteOpen: false,
    });

    try {
      const content = await invoke<FileContent>("read_file", {
        sessionId: sessionId ?? null,
        repoId: repoId ?? null,
        filePath,
      });
      set({ content, loading: false });
    } catch (e) {
      set({ error: String(e), loading: false });
    }
  },

  closeViewer: () => {
    set({
      isOpen: false,
      filePath: null,
      content: null,
      error: null,
      scrollToLine: null,
      highlightRange: null,
    });
  },

  openPalette: () => {
    set({ isPaletteOpen: true, searchQuery: "", searchResults: [], searchLoading: false });
  },

  closePalette: () => {
    set({ isPaletteOpen: false, searchQuery: "", searchResults: [] });
  },

  searchFiles: async ({ sessionId, repoId, query }) => {
    set({ searchQuery: query, searchLoading: true });
    try {
      const results = await invoke<FileMatch[]>("search_files", {
        sessionId: sessionId ?? null,
        repoId: repoId ?? null,
        query,
      });
      // Only update if query hasn't changed (prevent stale results)
      if (get().searchQuery === query) {
        set({ searchResults: results, searchLoading: false });
      }
    } catch {
      set({ searchResults: [], searchLoading: false });
    }
  },
}));
```

- [ ] **Step 2: Verify build**

Run: `bun run build`
Expected: Build succeeds.

- [ ] **Step 3: Commit**

```bash
git add src/stores/fileViewerStore.ts
git commit -m "feat(file): add file viewer Zustand store"
```

---

## Chunk 3: FileViewer Overlay Component

### Task 8: Create the FileViewer overlay component

**Files:**
- Create: `src/components/FileViewer/FileViewer.tsx`

- [ ] **Step 1: Create the FileViewer component**

Create `src/components/FileViewer/FileViewer.tsx`:

```tsx
import { useEffect, useRef, useState, useCallback } from "react";
import { useFileViewerStore } from "../../stores/fileViewerStore";
import { codeToHtml } from "shiki";
import { useSessionStore } from "../../stores/sessionStore";
import { useShallow } from "zustand/react/shallow";

export function FileViewer() {
  const { isOpen, content, loading, error, filePath, scrollToLine, highlightRange, closeViewer } =
    useFileViewerStore();
  const activeSession = useSessionStore(
    useShallow((s) => s.getActiveSession()),
  );

  const codeRef = useRef<HTMLDivElement>(null);
  const [highlightedHtml, setHighlightedHtml] = useState<string>("");
  const [isVisible, setIsVisible] = useState(false);

  // Search state
  const [searchOpen, setSearchOpen] = useState(false);
  const [searchQuery, setSearchQuery] = useState("");
  const [matchIndex, setMatchIndex] = useState(0);
  const [matchCount, setMatchCount] = useState(0);
  const searchInputRef = useRef<HTMLInputElement>(null);

  // Jump to line state
  const [jumpOpen, setJumpOpen] = useState(false);
  const [jumpValue, setJumpValue] = useState("");
  const jumpInputRef = useRef<HTMLInputElement>(null);

  // Animate in/out
  useEffect(() => {
    if (isOpen) {
      requestAnimationFrame(() => setIsVisible(true));
    } else {
      setIsVisible(false);
    }
  }, [isOpen]);

  // Syntax highlight with Shiki
  useEffect(() => {
    if (!content) return;

    let cancelled = false;
    codeToHtml(content.content, {
      lang: content.language,
      theme: "github-dark-default",
    })
      .then((html) => {
        if (!cancelled) setHighlightedHtml(html);
      })
      .catch(() => {
        // Fallback: plain text with line numbers
        if (!cancelled) {
          const lines = content.content.split("\n");
          const escaped = lines
            .map(
              (line, i) =>
                `<span class="line-number">${i + 1}</span>${escapeHtml(line)}`,
            )
            .join("\n");
          setHighlightedHtml(`<pre><code>${escaped}</code></pre>`);
        }
      });

    return () => {
      cancelled = true;
    };
  }, [content]);

  // Scroll to target line after rendering
  useEffect(() => {
    if (!highlightedHtml || !scrollToLine || !codeRef.current) return;

    const timer = setTimeout(() => {
      const lineEl = codeRef.current?.querySelector(
        `[data-line="${scrollToLine}"]`,
      );
      if (lineEl) {
        lineEl.scrollIntoView({ block: "center", behavior: "smooth" });
      } else {
        // Fallback: estimate scroll position based on line height
        const lineHeight = 20;
        codeRef.current?.scrollTo({
          top: (scrollToLine - 1) * lineHeight,
          behavior: "smooth",
        });
      }
    }, 50);

    return () => clearTimeout(timer);
  }, [highlightedHtml, scrollToLine]);

  // Keyboard shortcuts
  useEffect(() => {
    if (!isOpen) return;

    const handler = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.preventDefault();
        if (jumpOpen) {
          setJumpOpen(false);
        } else if (searchOpen) {
          setSearchOpen(false);
          setSearchQuery("");
        } else {
          closeViewer();
        }
        return;
      }

      // Cmd+F / Ctrl+F — open search
      if ((e.metaKey || e.ctrlKey) && e.key === "f") {
        e.preventDefault();
        setSearchOpen(true);
        setTimeout(() => searchInputRef.current?.focus(), 0);
        return;
      }

      // Ctrl+G — jump to line
      if (e.ctrlKey && e.key === "g") {
        e.preventDefault();
        setJumpOpen(true);
        setTimeout(() => jumpInputRef.current?.focus(), 0);
        return;
      }
    };

    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [isOpen, searchOpen, jumpOpen, closeViewer]);

  // Search logic
  const handleSearch = useCallback(
    (query: string) => {
      setSearchQuery(query);
      if (!codeRef.current || !query) {
        setMatchCount(0);
        setMatchIndex(0);
        return;
      }

      // Remove previous highlights
      codeRef.current
        .querySelectorAll(".search-highlight")
        .forEach((el) => {
          const parent = el.parentNode;
          if (parent) {
            parent.replaceChild(document.createTextNode(el.textContent || ""), el);
            parent.normalize();
          }
        });

      // Find and highlight matches in text nodes
      const walker = document.createTreeWalker(
        codeRef.current,
        NodeFilter.SHOW_TEXT,
      );
      const matches: Element[] = [];
      const lowerQuery = query.toLowerCase();
      const nodesToProcess: { node: Text; indices: number[] }[] = [];

      let node: Text | null;
      while ((node = walker.nextNode() as Text | null)) {
        const text = node.textContent || "";
        const lower = text.toLowerCase();
        const indices: number[] = [];
        let idx = lower.indexOf(lowerQuery);
        while (idx !== -1) {
          indices.push(idx);
          idx = lower.indexOf(lowerQuery, idx + 1);
        }
        if (indices.length > 0) {
          nodesToProcess.push({ node, indices });
        }
      }

      for (const { node: textNode, indices } of nodesToProcess) {
        const text = textNode.textContent || "";
        const parent = textNode.parentNode;
        if (!parent) continue;

        const frag = document.createDocumentFragment();
        let lastIdx = 0;

        for (const idx of indices) {
          if (idx > lastIdx) {
            frag.appendChild(document.createTextNode(text.slice(lastIdx, idx)));
          }
          const span = document.createElement("span");
          span.className = "search-highlight";
          span.textContent = text.slice(idx, idx + query.length);
          frag.appendChild(span);
          matches.push(span);
          lastIdx = idx + query.length;
        }

        if (lastIdx < text.length) {
          frag.appendChild(document.createTextNode(text.slice(lastIdx)));
        }

        parent.replaceChild(frag, textNode);
      }

      setMatchCount(matches.length);
      if (matches.length > 0) {
        setMatchIndex(0);
        matches[0].classList.add("search-current");
        matches[0].scrollIntoView({ block: "center" });
      }
    },
    [],
  );

  const navigateMatch = useCallback(
    (direction: 1 | -1) => {
      if (!codeRef.current || matchCount === 0) return;
      const highlights = codeRef.current.querySelectorAll(".search-highlight");
      highlights[matchIndex]?.classList.remove("search-current");
      const next = (matchIndex + direction + matchCount) % matchCount;
      setMatchIndex(next);
      highlights[next]?.classList.add("search-current");
      highlights[next]?.scrollIntoView({ block: "center" });
    },
    [matchIndex, matchCount],
  );

  // Jump to line
  const handleJump = useCallback(
    (lineStr: string) => {
      const line = parseInt(lineStr, 10);
      if (isNaN(line) || !codeRef.current) return;

      const lineHeight = 20;
      codeRef.current.scrollTo({
        top: (line - 1) * lineHeight,
        behavior: "smooth",
      });
      setJumpOpen(false);
      setJumpValue("");
    },
    [],
  );

  // Click to highlight line
  const handleCodeClick = useCallback((e: React.MouseEvent) => {
    const target = e.target as HTMLElement;
    const lineEl = target.closest("[data-line]");
    if (!lineEl) return;

    // Remove previous active
    codeRef.current
      ?.querySelectorAll(".line-active")
      .forEach((el) => el.classList.remove("line-active"));
    lineEl.classList.add("line-active");
  }, []);

  if (!isOpen) return null;

  return (
    <div
      className={`absolute inset-0 z-30 flex flex-col bg-surface-0/95 transition-opacity duration-150 ${isVisible ? "opacity-100" : "opacity-0"}`}
    >
      {/* Top bar */}
      <div className="flex items-center justify-between border-b border-surface-3 px-4 py-2">
        <div className="flex items-center gap-3 text-sm">
          <span className="text-zinc-300">{filePath}</span>
          {content && (
            <span className="text-zinc-500">
              {content.total_lines} lines · {content.language} · {content.encoding}
              {content.is_truncated && ` · showing first ${content.line_count}`}
            </span>
          )}
        </div>
        <div className="flex items-center gap-2 text-xs text-zinc-500">
          <kbd className="rounded bg-surface-2 px-1.5 py-0.5">⌘F</kbd>
          <kbd className="rounded bg-surface-2 px-1.5 py-0.5">Esc</kbd>
          <button
            onClick={closeViewer}
            className="ml-2 rounded p-1 text-zinc-400 hover:bg-surface-2 hover:text-zinc-200"
          >
            ✕
          </button>
        </div>
      </div>

      {/* Search bar */}
      {searchOpen && (
        <div className="flex items-center gap-2 border-b border-surface-3 px-4 py-1.5">
          <input
            ref={searchInputRef}
            type="text"
            value={searchQuery}
            onChange={(e) => handleSearch(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter") {
                e.preventDefault();
                navigateMatch(e.shiftKey ? -1 : 1);
              }
              if (e.key === "Escape") {
                e.preventDefault();
                setSearchOpen(false);
                setSearchQuery("");
              }
            }}
            placeholder="Search in file..."
            className="w-64 rounded bg-surface-2 px-2 py-1 text-sm text-zinc-200 outline-none focus:ring-1 focus:ring-accent"
          />
          {matchCount > 0 && (
            <>
              <span className="text-xs text-zinc-400">
                {matchIndex + 1}/{matchCount}
              </span>
              <button onClick={() => navigateMatch(-1)} className="text-zinc-400 hover:text-zinc-200">
                ↑
              </button>
              <button onClick={() => navigateMatch(1)} className="text-zinc-400 hover:text-zinc-200">
                ↓
              </button>
            </>
          )}
        </div>
      )}

      {/* Jump to line */}
      {jumpOpen && (
        <div className="flex items-center gap-2 border-b border-surface-3 px-4 py-1.5">
          <span className="text-xs text-zinc-400">Go to line:</span>
          <input
            ref={jumpInputRef}
            type="text"
            value={jumpValue}
            onChange={(e) => setJumpValue(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter") {
                e.preventDefault();
                handleJump(jumpValue);
              }
              if (e.key === "Escape") {
                e.preventDefault();
                setJumpOpen(false);
                setJumpValue("");
              }
            }}
            className="w-20 rounded bg-surface-2 px-2 py-1 text-sm text-zinc-200 outline-none focus:ring-1 focus:ring-accent"
          />
        </div>
      )}

      {/* Code area */}
      <div
        ref={codeRef}
        className="flex-1 overflow-auto"
        onClick={handleCodeClick}
      >
        {loading && (
          <div className="flex h-full items-center justify-center text-zinc-500">
            Loading...
          </div>
        )}
        {error && (
          <div className="flex h-full flex-col items-center justify-center gap-2 text-zinc-400">
            <p>{error}</p>
            <button
              onClick={closeViewer}
              className="rounded bg-surface-2 px-3 py-1 text-sm hover:bg-surface-3"
            >
              Dismiss
            </button>
          </div>
        )}
        {!loading && !error && highlightedHtml && (
          <div
            className="file-viewer-code p-4 text-[13px] leading-5"
            style={{ fontFamily: '"JetBrains Mono", "Fira Code", monospace' }}
            dangerouslySetInnerHTML={{ __html: highlightedHtml }}
          />
        )}
      </div>

      {/* Bottom status strip */}
      {activeSession && (
        <div className="border-t border-surface-3 px-4 py-1 text-xs text-zinc-500">
          <span className="mr-2">▪</span>
          <span>{activeSession.session.branch || "main"}</span>
          <span className="mx-1">·</span>
          <span>{activeSession.session.status}</span>
        </div>
      )}
    </div>
  );
}

function escapeHtml(text: string): string {
  return text
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;");
}
```

- [ ] **Step 2: Add CSS for search highlights and line active state**

Create `src/components/FileViewer/fileViewer.css`:

```css
.file-viewer-code .search-highlight {
  background-color: rgba(255, 200, 50, 0.3);
  border-radius: 2px;
}

.file-viewer-code .search-highlight.search-current {
  background-color: rgba(255, 200, 50, 0.6);
  outline: 1px solid rgba(255, 200, 50, 0.8);
}

.file-viewer-code .line-active {
  background-color: rgba(255, 255, 255, 0.05);
}

/* Shiki theme overrides to match Racc surface tokens */
.file-viewer-code pre {
  background-color: transparent !important;
  margin: 0;
}

.file-viewer-code code {
  counter-reset: line;
}

.file-viewer-code code .line::before {
  counter-increment: line;
  content: counter(line);
  display: inline-block;
  width: 3em;
  margin-right: 1em;
  text-align: right;
  color: #52525b;
  user-select: none;
  position: sticky;
  left: 0;
}
```

- [ ] **Step 3: Import CSS in the component**

Add to the top of `FileViewer.tsx` (after the other imports):

```typescript
import "./fileViewer.css";
```

- [ ] **Step 4: Verify build**

Run: `bun run build`
Expected: Build succeeds.

- [ ] **Step 5: Commit**

```bash
git add src/components/FileViewer/
git commit -m "feat(file): add FileViewer overlay component with syntax highlighting and search"
```

---

## Chunk 4: Command Palette

### Task 9: Create the CommandPalette component

**Files:**
- Create: `src/components/FileViewer/CommandPalette.tsx`

- [ ] **Step 1: Create the CommandPalette component**

Create `src/components/FileViewer/CommandPalette.tsx`:

```tsx
import { useEffect, useRef, useState } from "react";
import { useFileViewerStore } from "../../stores/fileViewerStore";
import { useSessionStore } from "../../stores/sessionStore";
import { useShallow } from "zustand/react/shallow";

export function CommandPalette() {
  const { isPaletteOpen, searchResults, searchLoading, closePalette, searchFiles, openFile } =
    useFileViewerStore();
  const activeSession = useSessionStore(useShallow((s) => s.getActiveSession()));
  const repos = useSessionStore((s) => s.repos);

  const inputRef = useRef<HTMLInputElement>(null);
  const [query, setQuery] = useState("");
  const [selectedIndex, setSelectedIndex] = useState(0);

  // Resolve session/repo context
  const sessionId = activeSession?.session.id ?? null;
  const repoId = activeSession?.repo.id ?? (repos.length === 1 ? repos[0].repo.id : null);
  const hasContext = sessionId !== null || repoId !== null;

  // Focus input on open
  useEffect(() => {
    if (isPaletteOpen) {
      setQuery("");
      setSelectedIndex(0);
      setTimeout(() => inputRef.current?.focus(), 0);
    }
  }, [isPaletteOpen]);

  // Debounced search
  useEffect(() => {
    if (!isPaletteOpen || !hasContext) return;

    const timer = setTimeout(() => {
      searchFiles({ sessionId, repoId, query });
    }, 100);

    return () => clearTimeout(timer);
  }, [query, isPaletteOpen, sessionId, repoId, hasContext, searchFiles]);

  // Reset selection when results change
  useEffect(() => {
    setSelectedIndex(0);
  }, [searchResults]);

  const selectFile = (filePath: string) => {
    openFile({ sessionId, repoId, filePath });
  };

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === "Escape") {
      e.preventDefault();
      closePalette();
      return;
    }

    if (e.key === "ArrowDown") {
      e.preventDefault();
      setSelectedIndex((i) => Math.min(i + 1, searchResults.length - 1));
      return;
    }

    if (e.key === "ArrowUp") {
      e.preventDefault();
      setSelectedIndex((i) => Math.max(i - 1, 0));
      return;
    }

    if (e.key === "Enter" && searchResults.length > 0) {
      e.preventDefault();
      selectFile(searchResults[selectedIndex].relative_path);
      return;
    }
  };

  if (!isPaletteOpen) return null;

  return (
    <div className="fixed inset-0 z-40 flex items-start justify-center pt-[15%]">
      {/* Backdrop */}
      <div className="fixed inset-0 bg-black/50" onClick={closePalette} />

      {/* Palette */}
      <div className="relative w-full max-w-lg rounded-lg border border-surface-3 bg-surface-1 shadow-2xl">
        {/* Input */}
        <div className="flex items-center border-b border-surface-3 px-3">
          <span className="text-zinc-500 text-sm">&gt;</span>
          <input
            ref={inputRef}
            type="text"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            onKeyDown={handleKeyDown}
            placeholder={hasContext ? "Search files..." : "Select a repo first"}
            disabled={!hasContext}
            className="w-full bg-transparent px-2 py-2.5 text-sm text-zinc-200 outline-none placeholder:text-zinc-600"
          />
          {searchLoading && (
            <span className="text-xs text-zinc-500">...</span>
          )}
        </div>

        {/* Results */}
        {hasContext && searchResults.length > 0 && (
          <div className="max-h-64 overflow-auto py-1">
            {searchResults.map((result, i) => (
              <button
                key={result.relative_path}
                onClick={() => selectFile(result.relative_path)}
                className={`flex w-full items-center px-3 py-1.5 text-left text-sm ${
                  i === selectedIndex
                    ? "bg-accent/20 text-zinc-100"
                    : "text-zinc-400 hover:bg-surface-2 hover:text-zinc-200"
                }`}
              >
                <span className="truncate">{result.relative_path}</span>
              </button>
            ))}
          </div>
        )}

        {/* No results */}
        {hasContext && query && !searchLoading && searchResults.length === 0 && (
          <div className="px-3 py-4 text-center text-sm text-zinc-500">
            No files found
          </div>
        )}

        {/* No context */}
        {!hasContext && (
          <div className="px-3 py-4 text-center text-sm text-zinc-500">
            Select a repo first to search files
          </div>
        )}
      </div>
    </div>
  );
}
```

- [ ] **Step 2: Verify build**

Run: `bun run build`
Expected: Build succeeds.

- [ ] **Step 3: Commit**

```bash
git add src/components/FileViewer/CommandPalette.tsx
git commit -m "feat(file): add command palette for fuzzy file search"
```

---

### Task 10: Integrate FileViewer and CommandPalette into App.tsx

**Files:**
- Modify: `src/App.tsx`

- [ ] **Step 1: Add components and keyboard shortcut to App.tsx**

In `src/App.tsx`, add imports at the top:

```typescript
import { FileViewer } from "./components/FileViewer/FileViewer";
import { CommandPalette } from "./components/FileViewer/CommandPalette";
import { useFileViewerStore } from "./stores/fileViewerStore";
```

Add a `useEffect` for the `Cmd+P` global shortcut (inside the `App` component, after the existing `useEffect`):

```typescript
  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key === "p") {
        e.preventDefault();
        useFileViewerStore.getState().openPalette();
      }
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, []);
```

Add `<FileViewer />` and `<CommandPalette />` inside the layout. The `<main>` element that wraps `<Terminal />` needs `relative` positioning so the overlay positions correctly. Add the components inside `<main>`:

The `<main>` tag needs `relative` added (preserve existing classes like `border-x border-surface-3`):

```tsx
<main className="relative flex flex-1 flex-col border-x border-surface-3">
  <Terminal />
  <FileViewer />
</main>
```

`CommandPalette` must be rendered **outside** `<main>` at the root level (sibling to the flex container) so its backdrop covers the entire viewport:

```tsx
<div className="flex h-screen flex-col bg-surface-0">
  <div className="flex flex-1 overflow-hidden">
    <Sidebar />
    <main className="relative flex flex-1 flex-col border-x border-surface-3">
      <Terminal />
      <FileViewer />
    </main>
    <aside className="w-80">
      <CostTracker />
      <AssistantPanel />
    </aside>
  </div>
  <StatusBar />
  <CommandPalette />
</div>
```

- [ ] **Step 2: Verify build**

Run: `bun run build`
Expected: Build succeeds.

- [ ] **Step 3: Manual verification**

Run: `bun tauri dev`
- Press `Cmd+P` → Command palette should appear
- Type a filename → fuzzy results should appear
- Select a file → overlay viewer should open with syntax highlighting
- Press `Cmd+F` → search bar should appear
- Press `Esc` → layers should dismiss in order (search → viewer)

- [ ] **Step 4: Commit**

```bash
git add src/App.tsx
git commit -m "feat(file): integrate file viewer and command palette into app layout"
```

---

## Chunk 5: Terminal Path Click & Pi Agent Integration

### Task 11: Add terminal path click support

**Files:**
- Modify: `src/components/Terminal/Terminal.tsx`

- [ ] **Step 1: Add link provider to Terminal component**

In `src/components/Terminal/Terminal.tsx`, after the xterm.js initialization (after `fitAddon.fit()` around line 50), add a link provider:

```typescript
// File path link provider for Cmd+Click / Ctrl+Click
xterm.registerLinkProvider({
  provideLinks(bufferLineNumber, callback) {
    const line = xterm.buffer.active.getLine(bufferLineNumber);
    if (!line) return callback(undefined);

    const text = line.translateToString();
    // Match common file paths (relative paths with extensions, optionally with :lineNumber)
    const pathRegex = /(?:^|\s)((?:[\w.-]+\/)*[\w.-]+\.\w+)(?::(\d+))?/g;
    const links: Array<{
      range: { start: { x: number; y: number }; end: { x: number; y: number } };
      text: string;
      activate: (e: MouseEvent, text: string) => void;
    }> = [];

    let match;
    while ((match = pathRegex.exec(text)) !== null) {
      const fullMatch = match[0].trimStart();
      const filePath = match[1];
      const lineNum = match[2] ? parseInt(match[2], 10) : undefined;
      const startX = text.indexOf(fullMatch, match.index) + 1; // 1-based

      links.push({
        range: {
          start: { x: startX, y: bufferLineNumber + 1 },
          end: { x: startX + fullMatch.length, y: bufferLineNumber + 1 },
        },
        text: fullMatch,
        activate: (_e: MouseEvent) => {
          const { openFile } = useFileViewerStore.getState();
          const activeSession = useSessionStore.getState().getActiveSession();
          if (activeSession) {
            openFile({
              sessionId: activeSession.session.id,
              repoId: activeSession.repo.id,
              filePath,
              scrollToLine: lineNum,
            });
          }
        },
      });
    }

    callback(links.length > 0 ? links : undefined);
  },
});
```

Add the required import at the top of `Terminal.tsx`:

```typescript
import { useFileViewerStore } from "../../stores/fileViewerStore";
```

Note: `useSessionStore` is already imported in `Terminal.tsx` — no need to add it again.

- [ ] **Step 2: Verify build**

Run: `bun run build`
Expected: Build succeeds.

- [ ] **Step 3: Commit**

```bash
git add src/components/Terminal/Terminal.tsx
git commit -m "feat(file): add terminal path click to open file viewer"
```

---

### Task 12: Add inline code preview to Pi Agent chat

**Files:**
- Modify: `src/components/Assistant/AssistantMessage.tsx`
- Modify: `src/components/Assistant/AssistantChat.tsx`

- [ ] **Step 1: Add "Open Full File" button to code blocks in AssistantMessage**

Modify `src/components/Assistant/AssistantMessage.tsx` to detect code blocks with file path headers and add an "Open Full File" button.

Add imports at top:

```typescript
import { useFileViewerStore } from "../../stores/fileViewerStore";
import { useSessionStore } from "../../stores/sessionStore";
```

Add a helper component and modify the message rendering to include a button after code blocks that reference files. After the `ReactMarkdown` component, add a callback to detect file references in the message content and render a button:

```tsx
function OpenFileButton({ content }: { content: string }) {
  // Match pattern: "filename.ext · Lines X-Y (Z total)" or just "filename.ext"
  const fileMatch = content.match(/^(\S+\.\w+)\s*·/m);
  if (!fileMatch) return null;

  const filePath = fileMatch[1];

  const handleClick = () => {
    const activeSession = useSessionStore.getState().getActiveSession();
    if (!activeSession) return;

    // Try to extract line number from "Lines X-Y" pattern
    const lineMatch = content.match(/Lines?\s+(\d+)/i);
    const scrollToLine = lineMatch ? parseInt(lineMatch[1], 10) : undefined;

    useFileViewerStore.getState().openFile({
      sessionId: activeSession.session.id,
      repoId: activeSession.repo.id,
      filePath,
      scrollToLine,
    });
  };

  return (
    <button
      onClick={handleClick}
      className="mt-1 text-xs text-accent hover:text-accent-hover"
    >
      [ Open Full File ↗ ]
    </button>
  );
}
```

In the assistant message rendering, add `<OpenFileButton content={message.content} />` after the `ReactMarkdown` block for assistant messages.

- [ ] **Step 2: Verify build**

Run: `bun run build`
Expected: Build succeeds.

- [ ] **Step 3: Commit**

```bash
git add src/components/Assistant/AssistantMessage.tsx
git commit -m "feat(file): add open-full-file button to assistant code blocks"
```

---

### Task 13: Final integration verification

- [ ] **Step 1: Full build check**

Run: `bun run build`
Expected: TypeScript checks pass, Vite build succeeds.

- [ ] **Step 2: Rust build check**

Run: `cd src-tauri && cargo check`
Expected: All Rust code compiles.

- [ ] **Step 3: Full app verification**

Run: `bun tauri dev`

Verify all three trigger mechanisms:
1. **Cmd+P** → opens palette → type filename → select → overlay opens with highlighted code
2. **Terminal path click** → Cmd+Click a file path in agent output → overlay opens
3. **Pi Agent** → ask "show me session.rs" → inline code appears → click "Open Full File ↗" → overlay opens

Verify overlay features:
4. **Cmd+F** → search bar → type query → highlights appear → Enter navigates matches
5. **Ctrl+G** → jump to line input → enter line number → scrolls
6. **Esc** → dismisses layers in order (search → viewer)
7. **Click a line** → line highlights
8. Sidebar remains visible and shows agent status during overlay

- [ ] **Step 4: Commit any fixes**

```bash
git add -A
git commit -m "fix(file): address integration issues from manual testing"
```
