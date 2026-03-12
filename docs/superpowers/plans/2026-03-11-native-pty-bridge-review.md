# Review: Native PTY Bridge Implementation Plan

**Reviewer:** Claude Opus 4.6 (Senior Code Reviewer)
**Date:** 2026-03-11
**Plan file:** `docs/superpowers/plans/2026-03-11-native-pty-bridge.md`

---

## Summary

The plan is well-structured and covers the core migration from tmux polling to native PTY streaming. Task ordering respects dependencies, and the DB migration approach is sound. However, there are several issues ranging from a critical API inaccuracy to missing edge cases.

---

## Critical Issues

### 1. `process.env.SHELL` is not available in Tauri WebView context

**File in plan:** `src/services/ptyManager.ts`, line inside `getShell()`

```typescript
return platform() === "windows" ? "powershell.exe" : process.env.SHELL || "/bin/zsh";
```

`process.env` does not exist in a browser/WebView context. This will throw a ReferenceError at runtime. The shell must be detected on the Rust side and passed to the frontend, or use a Tauri environment API. Alternatively, hard-code `/bin/zsh` for macOS (the only current target per the project) or read from the OS plugin's `locale`/`env` facilities.

### 2. `tauri-plugin-pty` JS API shape is unverified

The plan assumes this exact API surface from `tauri-pty`:
- `spawn(shell, args, { cols, rows, cwd, env })` returning a `PtyProcess`
- `PtyProcess.onData(cb)`, `PtyProcess.onExit(cb)`, `PtyProcess.write(data)`, `PtyProcess.resize(cols, rows)`, `PtyProcess.kill()`

`tauri-plugin-pty` is a community plugin (not an official Tauri plugin). Its actual API may differ. The named import `import { spawn, type PtyProcess } from "tauri-pty"` and the exact callback signatures (`onData` receiving `Uint8Array`, `onExit` receiving `{ exitCode }`) must be validated against the actual package. If the API does not match, the entire `ptyManager.ts` and `usePtyBridge.ts` will fail to compile or function.

**Recommendation:** Before implementation, run `bun add tauri-pty` and inspect the exported types from the package. Add a verification step as Task 0.

### 3. DB migration does not run inside a transaction

**File in plan:** `src-tauri/src/commands/db.rs`, migration v2

`execute_batch` runs multiple statements but if it fails partway through (e.g., after `DROP TABLE sessions` but before `ALTER TABLE`), the database is left in a corrupt state with no `sessions` table. The v1 migration has the same pattern so this is a pre-existing risk, but v2 is more dangerous because it performs destructive operations (DROP TABLE) on user data.

**Recommendation:** Wrap in an explicit transaction:
```rust
conn.execute_batch("BEGIN; ... COMMIT;")?;
```
Or use `conn.transaction()` from rusqlite.

---

## Important Warnings

### 4. Plan does not mention `@tauri-apps/plugin-os` Rust dependency registration

Task 6 Step 2 says to add `@tauri-apps/plugin-os` and register `tauri_plugin_os::init()`, but this is buried inside a PTY manager task rather than being its own explicit step. It also requires adding `tauri-plugin-os = "2"` to `Cargo.toml` and `"os:default"` to `capabilities/default.json`. These three changes across different files should be a distinct step or at minimum called out more prominently. Given issue #1 above, this dependency may not even be needed if `process.env.SHELL` is replaced with a different approach.

### 5. PTY spawn uses hardcoded terminal dimensions (120x30)

**File in plan:** `src/stores/sessionStore.ts`

```typescript
spawnPty(session.id, cwd, 120, 30, "claude");
```

The terminal may not be mounted yet when `createSession` is called from the store. The PTY is created with 120x30, but the actual xterm.js dimensions are unknown until the component renders and `FitAddon.fit()` runs. The `usePtyBridge` hook does call `resizePty` on mount, so there will be a resize shortly after, but the initial output from the shell and the `claude` command may render incorrectly in the buffer.

**Recommendation:** Either defer PTY spawn until the terminal component is ready (via a callback or event), or accept the mismatch and document that the initial resize corrects it.

### 6. No cleanup of PTYs on app close / window unload

The plan mentions `killAll()` exists in `ptyManager.ts` but never wires it up. If the app is closed while sessions are running, PTY child processes may become orphaned.

**Recommendation:** Add a `beforeunload` handler or Tauri window close hook that calls `killAll()`. This could be in `App.tsx` or a dedicated cleanup effect.

### 7. `reconcile_sessions` marks ALL Running sessions as Disconnected

This is correct behavior (PTY state is in-memory and lost on restart), and the plan documents it well. However, there is no mechanism to **re-attach** to a disconnected session. If a user restarts the app, all their sessions become Disconnected with no way to spawn a new PTY in the same worktree/context. Consider adding a "reconnect" action in a follow-up task.

### 8. Wiki/docs still reference tmux extensively

The grep found 15 markdown files referencing tmux, including `wiki/Session-Lifecycle.md`, `wiki/Technical-Architecture.md`, `CLAUDE.md`, and others. The plan's Task 11 Step 4 only checks `src/` and `src-tauri/src/` for tmux references. Documentation and wiki files will become stale.

**Recommendation:** Add a step to update `CLAUDE.md` (which describes the "Session = tmux session + git worktree" convention) and the wiki files, or at minimum create a follow-up task for it.

---

## Notes (Nice to Have)

### 9. Buffer trimming strategy drops data at chunk granularity

In `ptyManager.ts`, when the buffer exceeds `MAX_BUFFER_SIZE`, it drops the oldest `Uint8Array` chunk. If a single chunk is very large, the buffer could remain well over the limit. A byte-precise trim would be more predictable, but this is acceptable for a v1.

### 10. `setTimeout` for agent command is fragile

```typescript
setTimeout(() => { pty.write(agentCmd + "\n"); }, 100);
```

A 100ms delay may not be sufficient for the shell to initialize on slower machines. Consider listening for the first `onData` event (indicating the shell has produced a prompt) before sending the agent command.

### 11. Task ordering note: Task 2 may need to run before Task 1 Step 5

The plan acknowledges this: "If `cargo check` fails because of the missing tmux handlers, do Task 2 first." This conditional ordering is fine, but it would be cleaner to simply reorder Tasks 1 and 2 so that tmux module removal happens before the lib.rs handler list is changed, avoiding the potential compile failure entirely.

### 12. `git add -A` usage in commit steps

Task 2 Step 4 uses `git add -A src-tauri/src/commands/tmux.rs` which stages deletions correctly. However, Task 11 Step 6 uses `git add -A` (global) which could accidentally stage unrelated files. This is minor since it is a fixup commit, but worth noting.

---

## Completeness Checklist

| Concern | Covered? | Notes |
|---------|----------|-------|
| Rust dependencies (Cargo.toml) | Yes | |
| JS dependencies (package.json) | Yes | |
| Plugin registration (lib.rs) | Yes | |
| Capabilities/permissions | Yes | |
| DB migration (drop tmux column) | Yes | Needs transaction wrapping |
| Session struct refactor (Rust) | Yes | |
| Session type refactor (TS) | Yes | |
| tmux module deletion | Yes | |
| PTY manager service | Yes | API needs verification |
| Bridge hook | Yes | |
| Terminal component update | Yes | |
| Session store integration | Yes | |
| useTmuxBridge deletion | Yes | |
| Build verification | Yes | |
| Smoke test checklist | Yes | |
| App close cleanup | No | Missing |
| Wiki/docs update | No | Missing |
| `process.env` in WebView | No | Will crash at runtime |

---

## Verdict

The plan is **not ready for implementation as-is**. The three critical issues must be resolved first:

1. Fix `process.env.SHELL` usage in `ptyManager.ts` -- this will crash at runtime
2. Verify `tauri-plugin-pty` / `tauri-pty` actual API before coding against assumed signatures
3. Wrap DB migration v2 in an explicit transaction to prevent data loss on partial failure

After those fixes, the plan is solid and well-organized for execution.
