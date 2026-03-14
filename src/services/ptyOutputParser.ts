import { subscribe } from "./ptyManager";

// Strip ANSI escape sequences from terminal output
function stripAnsi(str: string): string {
  return str.replace(/\x1b\[[0-9;]*[a-zA-Z]/g, "");
}

type OutputCallback = (sessionId: number, lastLine: string) => void;

interface TrackedSession {
  sessionId: number;
  unsubscribe: (() => void) | null;
  decoder: TextDecoder;
}

const tracked = new Map<number, TrackedSession>();
let onOutputUpdate: OutputCallback | null = null;

/** Set the callback that receives output updates. Call once at app init. */
export function setOutputCallback(cb: OutputCallback): void {
  onOutputUpdate = cb;
}

function handlePtyData(sessionId: number, data: Uint8Array): void {
  const entry = tracked.get(sessionId);
  if (!entry || !onOutputUpdate) return;

  const decoded = stripAnsi(entry.decoder.decode(data, { stream: true }));
  if (!decoded.trim()) return;

  // Get the last non-empty line from this chunk
  const lines = decoded.split(/\r?\n/).filter((l) => l.trim().length > 0);
  if (lines.length === 0) return;

  const lastLine = lines[lines.length - 1].trim().slice(0, 120);
  onOutputUpdate(sessionId, lastLine);
}

/** Start tracking a session's PTY output. */
export function startTracking(sessionId: number): void {
  stopTracking(sessionId);

  const entry: TrackedSession = {
    sessionId,
    unsubscribe: null,
    decoder: new TextDecoder(),
  };

  tracked.set(sessionId, entry);

  const unsub = subscribe(sessionId, (data) => handlePtyData(sessionId, data));

  if (unsub) {
    entry.unsubscribe = unsub;
  } else {
    queueMicrotask(() => {
      const retryUnsub = subscribe(sessionId, (data) => handlePtyData(sessionId, data));
      if (retryUnsub) {
        entry.unsubscribe = retryUnsub;
      } else {
        console.warn(`[ptyOutputParser] subscribe failed for session ${sessionId}`);
        tracked.delete(sessionId);
      }
    });
  }
}

/** Stop tracking a session. */
export function stopTracking(sessionId: number): void {
  const entry = tracked.get(sessionId);
  if (!entry) return;
  if (entry.unsubscribe) entry.unsubscribe();
  tracked.delete(sessionId);
}
