import { subscribe } from "./ptyManager";

// Strip ANSI escape sequences from terminal output
function stripAnsi(str: string): string {
  return str.replace(/\x1b\[[0-9;]*[a-zA-Z]/g, "");
}

// Claude Code prompt markers — user input follows these
// Only match the Unicode prompt character ❯ (U+276F) to avoid false positives from > in diffs/markdown
const PROMPT_MARKER_PATTERN = /^❯\s+(.+)/;
const HUMAN_TURN_PATTERN = /^\s*Human:\s*$/;

type OutputCallback = (sessionId: number, lastLine: string) => void;

interface TrackedSession {
  sessionId: number;
  unsubscribe: (() => void) | null;
  decoder: TextDecoder;
  promptCount: number;
  inHumanTurn: boolean;
  lineBuffer: string;
}

const tracked = new Map<number, TrackedSession>();
let onOutputUpdate: OutputCallback | null = null;

type PromptCallback = (sessionId: number, text: string, position: number) => void;
let onPromptDetected: PromptCallback | null = null;

/** Set the callback that receives user prompt detections. */
export function setPromptCallback(cb: PromptCallback): void {
  onPromptDetected = cb;
}

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

  // Detect user prompts from PTY output (for insights event capture)
  if (onPromptDetected) {
    for (const line of lines) {
      const trimmed = line.trim();
      if (!trimmed) continue;

      if (HUMAN_TURN_PATTERN.test(trimmed)) {
        entry.inHumanTurn = true;
        entry.lineBuffer = "";
        continue;
      }

      const promptMatch = trimmed.match(PROMPT_MARKER_PATTERN);
      if (promptMatch) {
        const text = promptMatch[1].trim();
        if (text.length > 5) {
          entry.promptCount++;
          onPromptDetected(sessionId, text, entry.promptCount);
        }
        entry.inHumanTurn = false;
        entry.lineBuffer = "";
      }
    }
  }
}

/** Start tracking a session's PTY output. */
export function startTracking(sessionId: number): void {
  stopTracking(sessionId);

  const entry: TrackedSession = {
    sessionId,
    unsubscribe: null,
    decoder: new TextDecoder(),
    promptCount: 0,
    inHumanTurn: false,
    lineBuffer: "",
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
