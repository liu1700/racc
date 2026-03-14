import { subscribe } from "./ptyManager";
import type { SessionActivity } from "../types/session";

const PARSER_BUFFER_LINES = 100;
const IDLE_TIMEOUT_MS = 10_000;

// Strip ANSI escape sequences from terminal output
function stripAnsi(str: string): string {
  return str.replace(/\x1b\[[0-9;]*[a-zA-Z]/g, "");
}

// --- Claude Code pattern matchers ---

// Tool use patterns: ⏺ Read, ⏺ Edit, ⏺ Write, ⏺ Bash, etc.
// The ⏺ character may appear with surrounding ANSI codes, so we match after stripping.
const TOOL_PATTERN = /[⏺●]\s*(Read|Edit|Write|Bash|Search|Glob|Grep|Agent)\b/;
const FILE_PATH_PATTERN = /(?:^|\s)((?:\/|\.\.?\/|src\/|tests?\/)\S+)/;
const BASH_CMD_PATTERN = /[⏺●]\s*Bash\b[^]*?(?:\$|>)\s*(.+)/;
const PERMISSION_PATTERN = /(?:Allow|Do you want to|Approve|Yes\/No|allow this)/i;
const EXIT_PATTERN = /\[Process exited with code (\d+)\]/;

// Claude Code prompt markers — user input follows these
// Only match the Unicode prompt character ❯ (U+276F) to avoid false positives from > in diffs/markdown
const PROMPT_MARKER_PATTERN = /^❯\s+(.+)/;
const HUMAN_TURN_PATTERN = /^\s*Human:\s*$/;

type ActivityCallback = (sessionId: number, activity: SessionActivity) => void;

interface TrackedSession {
  sessionId: number;
  agent: string;
  lines: string[];
  unsubscribe: (() => void) | null;
  lastActivityTime: number;
  idleTimer: ReturnType<typeof setTimeout> | null;
  decoder: TextDecoder;
  promptCount: number;
  inHumanTurn: boolean;
  lineBuffer: string;
}

const tracked = new Map<number, TrackedSession>();
let onActivityUpdate: ActivityCallback | null = null;

type PromptCallback = (sessionId: number, text: string, position: number) => void;
let onPromptDetected: PromptCallback | null = null;

/** Set the callback that receives user prompt detections. */
export function setPromptCallback(cb: PromptCallback): void {
  onPromptDetected = cb;
}

/** Set the callback that receives activity updates. Call once at app init. */
export function setActivityCallback(cb: ActivityCallback): void {
  onActivityUpdate = cb;
}

function emitActivity(sessionId: number, action: string, detail: string | null): void {
  if (!onActivityUpdate) return;

  const entry = tracked.get(sessionId);
  if (entry) {
    entry.lastActivityTime = Date.now();

    // Reset idle timer (but not when emitting Idle itself to avoid infinite loop)
    if (entry.idleTimer) clearTimeout(entry.idleTimer);
    if (action !== "Idle") {
      entry.idleTimer = setTimeout(() => {
        emitActivity(sessionId, "Idle", null);
      }, IDLE_TIMEOUT_MS);
    }
  }

  onActivityUpdate(sessionId, {
    sessionId,
    action,
    detail,
    timestamp: Date.now(),
  });
}

function parseClaudeCodeOutput(_lines: string[], latestChunk: string): { action: string; detail: string | null } | null {
  // Check latest chunk first for most recent activity

  // Permission prompt
  if (PERMISSION_PATTERN.test(latestChunk)) {
    return { action: "Waiting for approval", detail: null };
  }

  // Process exit
  const exitMatch = latestChunk.match(EXIT_PATTERN);
  if (exitMatch) {
    return { action: "Completed", detail: `exit ${exitMatch[1]}` };
  }

  // Tool use
  const toolMatch = latestChunk.match(TOOL_PATTERN);
  if (toolMatch) {
    const tool = toolMatch[1];

    switch (tool) {
      case "Read": {
        const fileMatch = latestChunk.match(FILE_PATH_PATTERN);
        return { action: "Reading", detail: fileMatch?.[1]?.slice(0, 60) ?? null };
      }
      case "Edit": {
        const fileMatch = latestChunk.match(FILE_PATH_PATTERN);
        return { action: "Editing", detail: fileMatch?.[1]?.slice(0, 60) ?? null };
      }
      case "Write": {
        const fileMatch = latestChunk.match(FILE_PATH_PATTERN);
        return { action: "Writing", detail: fileMatch?.[1]?.slice(0, 60) ?? null };
      }
      case "Bash": {
        const cmdMatch = latestChunk.match(BASH_CMD_PATTERN);
        const cmd = cmdMatch?.[1]?.trim().slice(0, 40) ?? null;
        return { action: "Running command", detail: cmd };
      }
      case "Search":
      case "Glob":
      case "Grep": {
        const pathMatch = latestChunk.match(FILE_PATH_PATTERN);
        return { action: "Searching", detail: pathMatch?.[1]?.slice(0, 60) ?? null };
      }
      case "Agent": {
        return { action: "Running agent", detail: null };
      }
    }
  }

  // Thinking — check for common spinner/thinking indicators
  // Claude Code shows a spinner or "Thinking..." text
  if (/thinking|\.{3,}$/i.test(latestChunk.trim())) {
    return { action: "Thinking", detail: null };
  }

  return null;
}

type AgentParser = (lines: string[], latestChunk: string) => { action: string; detail: string | null } | null;

const parsers: Record<string, AgentParser> = {
  "claude-code": parseClaudeCodeOutput,
};

function handlePtyData(sessionId: number, data: Uint8Array): void {
  const entry = tracked.get(sessionId);
  if (!entry) return;

  const decoded = stripAnsi(entry.decoder.decode(data, { stream: true }));
  if (!decoded.trim()) return;

  // Split into lines and add to buffer
  const newLines = decoded.split(/\r?\n/);
  entry.lines.push(...newLines);

  // Trim buffer to max size
  if (entry.lines.length > PARSER_BUFFER_LINES) {
    entry.lines.splice(0, entry.lines.length - PARSER_BUFFER_LINES);
  }

  // Run parser
  const parser = parsers[entry.agent];
  if (!parser) return;

  const result = parser(entry.lines, decoded);
  if (result) {
    emitActivity(sessionId, result.action, result.detail);
  }

  // Detect user prompts from PTY output
  if (onPromptDetected) {
    for (const line of newLines) {
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

/** Start tracking a session's PTY output. Call AFTER spawnPty() has returned. */
export function startTracking(sessionId: number, agent: string): void {
  // Clean up if already tracking
  stopTracking(sessionId);

  const entry: TrackedSession = {
    sessionId,
    agent,
    lines: [],
    unsubscribe: null,
    lastActivityTime: Date.now(),
    idleTimer: null,
    decoder: new TextDecoder(),
    promptCount: 0,
    inHumanTurn: false,
    lineBuffer: "",
  };

  tracked.set(sessionId, entry);

  // Subscribe to PTY output
  const unsub = subscribe(sessionId, (data) => handlePtyData(sessionId, data));

  if (unsub) {
    entry.unsubscribe = unsub;
  } else {
    // PTY not yet registered — retry once after microtask
    console.warn(`[ptyOutputParser] subscribe returned null for session ${sessionId}, retrying...`);
    queueMicrotask(() => {
      const retryUnsub = subscribe(sessionId, (data) => handlePtyData(sessionId, data));
      if (retryUnsub) {
        entry.unsubscribe = retryUnsub;
      } else {
        console.warn(`[ptyOutputParser] subscribe still null for session ${sessionId}, giving up`);
        tracked.delete(sessionId);
      }
    });
  }

  // Emit initial activity (this also starts the idle timer via emitActivity)
  emitActivity(sessionId, "Starting", null);
}

/** Stop tracking a session. Cleans up listener and buffers. */
export function stopTracking(sessionId: number): void {
  const entry = tracked.get(sessionId);
  if (!entry) return;

  if (entry.unsubscribe) entry.unsubscribe();
  if (entry.idleTimer) clearTimeout(entry.idleTimer);
  tracked.delete(sessionId);
}
