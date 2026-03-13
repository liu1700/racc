# File Viewer Design Spec

## Overview

Racc is a control plane for AI coding agents, not a code editor. However, users need to **view files before deciding what instructions to give agents**. This spec adds a read-only file viewing capability that follows Racc's cognitive design principles: zero visual footprint when not in use, minimal cognitive mode-switching cost, and progressive disclosure.

## Priority

**P1** — Enhances the core agent orchestration workflow. Users need file context before giving agent instructions; without this, they must leave Racc entirely, breaking flow.

## Core Principle

**Zero-footprint, on-demand viewing.** No persistent file tree, no extra tabs, no always-visible UI. The viewer appears when needed and disappears completely when dismissed.

## Cognitive Science Foundation

- **Cowan's 4±1 limit**: The sidebar already uses 3 status categories. Adding a persistent file tree would push toward working memory limits. On-demand controls avoid this.
- **Information Foraging Theory** (Pirolli & Card): A persistent file tree has weak "information scent" — rarely needed but always consuming attention budget. On-demand triggers have zero cost when unused.
- **Attention switching cost (100-500ms)**: Each glance at an irrelevant UI region has a cost. Zero-footprint design eliminates cumulative cost of persistent elements.
- **Figure-Ground Segregation** (Gestalt): Semi-transparent overlays cause the brain to process two layers simultaneously. The overlay uses ~95% opacity to prevent visual interference.
- **Change Blindness**: Users won't notice terminal updates behind an overlay anyway, so transparency provides illusory benefit. Sidebar status colors (preattentive <200ms hue detection) provide genuine agent awareness.
- **Serial Search → Direct Access**: In-file search (`Cmd+F`) converts expensive serial visual scanning into direct access, critical for files with hundreds of lines.
- **Progressive Disclosure**: Small code snippets shown inline in Pi chat; full viewer only when needed. Information density escalates on demand.

## Trigger Mechanisms

Three ways to open file viewing, all with zero persistent UI:

### 1. Command Palette (`Cmd+P` / `Ctrl+P`)

- Global keyboard shortcut
- Centered fuzzy-search input box over the current layout
- Search scope:
  - If a session is active → that session's worktree directory
  - If only a repo is selected (no active session) → repo main directory
  - If neither session nor repo is selected → Command Palette is disabled (show hint: "Select a repo first")
- Respects `.gitignore` (no `node_modules`, build artifacts, etc.)
- Select a file → opens full overlay viewer
- `Esc` → dismisses palette, zero residual UI

### 2. Pi Agent (Natural Language)

- User asks Pi: "show me the routing config" or "what does create_session look like"
- Pi Agent calls a new `read_file` tool to fetch file contents
- **≤30 lines**: Pi shows an inline code snippet in the chat bubble (lightweight preview)
- **>30 lines** or user requests: Pi triggers the full overlay viewer
- Inline snippets always have a `[ Open Full File ↗ ]` button to upgrade

### 3. Terminal Path Click

- xterm.js link provider detects file paths in agent terminal output
- Paths rendered as underlined clickable links on `Cmd+Click` / `Ctrl+Click` (avoids accidental triggers)
- Click → opens full overlay viewer
- If path includes line number (e.g., `src/lib.rs:42`) → auto-scrolls to that line

## Lightweight Preview (Pi Chat Inline)

When Pi shows ≤30 lines of relevant code inside the chat:

### Display Rules

- If user asks about a specific function/logic → Pi extracts only the relevant snippet
- If user says "show me this file" without specifics → show first 30 lines + total line count
- Pi's explanation follows the code block (concrete → abstract cognitive processing order)

### Code Block Format

```
┌─ Pi Agent ──────────────────────────────────────┐
│                                                  │
│  session.rs · Lines 42-68 (210 total)            │
│  ┌────────────────────────────────────────────┐  │
│  │ 42  pub async fn create_session(           │  │
│  │ 43      app: AppHandle,                    │  │
│  │ ...                                        │  │
│  │ 68  }                                      │  │
│  └────────────────────────────────────────────┘  │
│                                                  │
│  Explanation text here...                        │
│                                                  │
│  [ Open Full File ↗ ]                            │
│                                                  │
└──────────────────────────────────────────────────┘
```

### Key Details

- **Real line numbers preserved**: User can reference specific lines when instructing agents
- **Header**: filename + line range + total lines — immediate spatial orientation
- **`[ Open Full File ↗ ]`**: Always present. Clicking opens overlay scrolled to the displayed line range with brief highlight

## Full Overlay Viewer

### Layout

```
+-- sidebar visible --+------ central overlay (covers terminal) ------+-- right panel visible --+
|                      |                                               |                         |
|  ● repo-1            |  ┌─ top bar ──────────────────────────────┐  |  Cost / Assistant       |
|    ● session-1       |  │ src/commands/session.rs                │  |  remains usable         |
|    ● session-2       |  │ 210 lines · Rust · UTF-8   [Cmd+F] [×]│  |                         |
|                      |  ├────────────────────────────────────────┤  |  User can view file     |
|  ● repo-2            |  │  1  use tauri::AppHandle;              │  |  AND ask Pi questions   |
|    ● session-3 🟢    |  │  2  use crate::db;                    │  |  simultaneously         |
|                      |  │  3                                     │  |                         |
|                      |  │ ...                                    │  |                         |
|                      |  │ 42  pub async fn create_session(       │  |                         |
|                      |  └────────────────────────────────────────┘  |                         |
|                      |  ▪ session-3 running · 12m                   |                         |
+----------------------+-----------------------------------------------+-------------------------+
```

### Top Bar

- **File path**: Relative to worktree root (no absolute paths — reduces noise)
- **Meta info**: Total lines · detected language · encoding (one line)
- **Shortcut hints**: `Cmd+F` search, `Esc` close
- If opened from Pi's `[ Open Full File ↗ ]`, auto-scrolls to the referenced line range with brief highlight animation

### Code Area

- **Syntax highlighting**: Shiki (VS Code-compatible TextMate grammars)
- **Line numbers**: Fixed left column, does not scroll horizontally
- **Click-to-highlight**: Clicking a line subtly highlights it (`surface-2` → `surface-3`), useful for referencing in Pi chat
- **Read-only**: No cursor blink, no edit capability — reinforces "viewing, not editing" mental model
- **Keyboard navigation**: Arrow keys for scrolling, `Page Up`/`Page Down` for page navigation, `Home`/`End` for top/bottom of file
- **Large file limit**: Files exceeding 10,000 lines show a warning with the option to load the first 10,000 lines. The `read_file` Rust command enforces this server-side to avoid large IPC payloads

### In-File Search (`Cmd+F`)

- Search bar slides down below top bar
- Real-time highlight of all matches
- Current match uses stronger highlight color (accent)
- Shows `current/total` count with `↑ ↓` navigation (or `Enter` / `Shift+Enter`)
- First `Esc` closes search bar; second `Esc` closes entire viewer

### Jump to Line (`Ctrl+G`)

- Small input box appears, type line number and press Enter
- Also used programmatically when Pi says "see line 42" or terminal path includes `:42`

### Bottom Status Strip

- Rendered inside the overlay as an internal footer, above the existing global `StatusBar` — not a modification to `StatusBar.tsx`
- One line showing active session state while terminal is covered
- Format: `▪ session-name · status · elapsed time`
- Uses existing status color tokens (preattentive channel preserved)

### Close Behavior

- `Esc` closes overlay with 150ms fade-out CSS transition
- Terminal underneath was never unmounted — it continues running during overlay display
- No state lost, instant return to terminal view

## File Reading Scope

No worktree creation required. File viewing is a pure read operation with zero side effects.

| Current State | Read Scope |
|---|---|
| Session selected | That session's existing worktree directory |
| Only repo selected, no active session | Repo's main directory |
| Neither selected | File viewing disabled; triggers show "Select a repo first" hint |

## Technical Implementation

### New Rust Commands

| Command | Signature | Purpose |
|---|---|---|
| `read_file` | `(path: String, max_lines: Option<usize>) -> Result<FileContent, String>` | Returns file content + metadata. Path validation enforced. Default max 10,000 lines |
| `search_files` | `(base_path: String, query: String) -> Result<Vec<FileMatch>, String>` | File traversal via `ignore` crate (respects `.gitignore`), fuzzy matching via `nucleo` crate. Returns top 20 ranked matches |

#### Return Types

```rust
#[derive(Debug, Serialize)]
struct FileContent {
    content: String,
    line_count: usize,
    total_lines: usize,    // actual file length (may exceed content if truncated)
    language: String,       // detected from file extension
    encoding: String,       // e.g., "utf-8"
    file_path: String,      // relative path to worktree/repo root
    is_truncated: bool,     // true if file exceeded max_lines
}

#[derive(Debug, Serialize)]
struct FileMatch {
    relative_path: String,  // relative to search base_path
    score: u32,             // fuzzy match score for ranking
    match_positions: Vec<usize>, // character positions of matches (for highlighting in UI)
}
```

#### Fuzzy Search Architecture

`search_files` uses two crates with distinct roles:
- **`ignore`**: Directory traversal respecting `.gitignore` rules — collects candidate file paths
- **`nucleo`**: Fuzzy subsequence matching on collected paths — ranks results by relevance

For large repos (100K+ files), file path listing is cached per worktree/repo and invalidated on session change. Fuzzy matching runs against the cached list on each keystroke.

### Security

- **Path traversal prevention**: Both commands validate that the resolved absolute path is within an imported repo or worktree directory. Reject paths containing `..` that escape the boundary.
- **Binary file detection**: Check file content before returning. Binary files show an informational message instead of garbled content.

### Keyboard Shortcut Conflicts

- `Cmd+F` is only captured when the `FileViewer` overlay is mounted. The handler calls `preventDefault()` to suppress the WebView's native find-in-page dialog.
- `Cmd+P` is captured globally at the app level. Tauri's WebView does not have a native handler for this shortcut, so no conflict exists.

### Frontend Components

| Component | Purpose |
|---|---|
| `CommandPalette` | `Cmd+P` fuzzy search overlay |
| `FileViewer` | Full overlay viewer with Shiki highlighting |
| `FileSearchBar` | `Cmd+F` in-file search within viewer |

### Syntax Highlighting

- **Library**: Shiki — static rendering, VS Code-compatible themes, no editor overhead
- **Why not CodeMirror**: CodeMirror includes editing infrastructure we don't need. Shiki is purpose-built for read-only rendering.

### Terminal Path Detection

- Use xterm.js built-in link provider API
- Register regex patterns for common file paths
- `Cmd+Click` / `Ctrl+Click` to activate (prevents accidental triggers from normal terminal interaction)

### Pi Agent Integration

- New Pi Agent tool: `read_file` — relayed to a Rust command `read_file_for_assistant` (following the existing `get_session_diff` → `get_session_diff_for_assistant` naming pattern)
- `read_file_for_assistant` wraps the same core logic as `read_file` but is registered as an assistant-callable command
- Pi decides inline vs overlay based on content length and user intent
- `[ Open Full File ↗ ]` button emits an event that the `FileViewer` component listens for

### State Management

New Zustand store `fileViewerStore.ts` shared across all trigger mechanisms:

```typescript
interface FileViewerState {
  isOpen: boolean;
  filePath: string | null;
  content: string | null;
  metadata: FileContent | null;
  scrollToLine: number | null;
  highlightRange: [number, number] | null; // [startLine, endLine]
  openFile: (path: string, scrollTo?: number, highlight?: [number, number]) => void;
  closeViewer: () => void;
}
```

All three triggers (Command Palette, Pi Agent button, Terminal Click) call `openFile()` to ensure consistent behavior.

### Error Handling

- **File not found** (deleted between search and open): Show inline message in overlay area "File not found — it may have been moved or deleted", with a dismiss button
- **Permission denied**: Show "Cannot read file — permission denied"
- **Encoding detection failure**: Fall back to raw UTF-8 display with a warning banner
- All errors are non-fatal — `Esc` always works to close and return to terminal

### Animation

- Overlay: 150ms CSS fade-in/fade-out transition
- No Motion library needed — simple opacity transition

### Shiki Bundle Optimization

- Use `shiki/bundle/web` with lazy-loaded grammars — load language grammar on first use rather than bundling all languages upfront
- Pre-load grammars for common languages (Rust, TypeScript, JavaScript, Python, Markdown) at app startup

## Explicitly Out of Scope

- ❌ File editing/saving — Racc is not an editor
- ❌ File tree / directory browser — violates zero-footprint principle
- ❌ Multiple file tabs — increases cognitive load; one file at a time
- ❌ Git blame / file history — exceeds "quick look before deciding" use case
- ❌ Worktree creation for viewing — read-only from existing directories
