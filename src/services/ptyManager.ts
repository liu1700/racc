import { spawn } from "tauri-pty";
import type { IPty, IDisposable } from "tauri-pty";

const MAX_BUFFER_SIZE = 1024 * 1024; // 1MB output buffer per session

// Default shell — process.env is not available in WebView context.
const DEFAULT_SHELL = "/bin/zsh";

interface PtyEntry {
  pty: IPty;
  buffer: Uint8Array[];
  bufferSize: number;
  listeners: Set<(data: Uint8Array) => void>;
  disposables: IDisposable[];
  exited: boolean;
  exitCode: number | null;
}

const entries = new Map<number, PtyEntry>();

export function spawnPty(
  sessionId: number,
  cwd: string,
  cols: number,
  rows: number,
  agentCmd?: string,
): void {
  if (entries.has(sessionId)) return;

  console.log("[ptyManager] spawnPty called:", { sessionId, cwd, cols, rows, agentCmd });
  const pty = spawn(DEFAULT_SHELL, [], {
    cols,
    rows,
    cwd,
    env: { TERM: "xterm-256color" },
  });
  console.log("[ptyManager] PTY spawned, pid:", pty.pid);

  const entry: PtyEntry = {
    pty,
    buffer: [],
    bufferSize: 0,
    listeners: new Set(),
    disposables: [],
    exited: false,
    exitCode: null,
  };

  const dataDisposable = pty.onData((data: Uint8Array) => {
    console.log("[ptyManager] onData for session", sessionId, "bytes:", data.length, "listeners:", entry.listeners.size);
    // Accumulate in buffer
    entry.buffer.push(data);
    entry.bufferSize += data.length;

    // Trim buffer if over max size (drop oldest chunks)
    while (entry.bufferSize > MAX_BUFFER_SIZE && entry.buffer.length > 1) {
      const dropped = entry.buffer.shift()!;
      entry.bufferSize -= dropped.length;
    }

    // Notify active listeners
    for (const listener of entry.listeners) {
      listener(data);
    }
  });

  const exitDisposable = pty.onExit(({ exitCode }) => {
    console.log("[ptyManager] onExit for session", sessionId, "exitCode:", exitCode);
    entry.exited = true;
    entry.exitCode = exitCode;
    const msg = new TextEncoder().encode(`\r\n[Process exited with code ${exitCode}]\r\n`);
    for (const listener of entry.listeners) {
      listener(msg);
    }
  });

  entry.disposables.push(dataDisposable, exitDisposable);
  entries.set(sessionId, entry);

  // Send agent command after a short delay to let shell initialize
  if (agentCmd) {
    setTimeout(() => {
      pty.write(agentCmd + "\n");
    }, 100);
  }
}

export function writePty(sessionId: number, data: string): void {
  entries.get(sessionId)?.pty.write(data);
}

export function resizePty(sessionId: number, cols: number, rows: number): void {
  entries.get(sessionId)?.pty.resize(cols, rows);
}

export function killPty(sessionId: number): void {
  const entry = entries.get(sessionId);
  if (!entry) return;
  for (const d of entry.disposables) d.dispose();
  if (!entry.exited) {
    entry.pty.kill();
  }
  entry.listeners.clear();
  entries.delete(sessionId);
}

/** Subscribe to live PTY output. Returns unsubscribe function. */
export function subscribe(
  sessionId: number,
  listener: (data: Uint8Array) => void,
): (() => void) | null {
  const entry = entries.get(sessionId);
  console.log("[ptyManager] subscribe called for session", sessionId, "entry exists:", !!entry);
  if (!entry) return null;
  entry.listeners.add(listener);
  console.log("[ptyManager] listener added, total listeners:", entry.listeners.size);
  return () => entry.listeners.delete(listener);
}

/** Get accumulated output buffer for replaying into xterm on session switch. */
export function getBuffer(sessionId: number): Uint8Array[] {
  return entries.get(sessionId)?.buffer ?? [];
}

/** Check if a PTY is alive for a given session. */
export function isAlive(sessionId: number): boolean {
  const entry = entries.get(sessionId);
  return entry !== undefined && !entry.exited;
}

/** Kill all PTYs (for app cleanup). */
export function killAll(): void {
  for (const [id] of entries) {
    killPty(id);
  }
}
