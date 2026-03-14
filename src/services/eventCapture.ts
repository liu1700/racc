import { invoke } from "@tauri-apps/api/core";
import { setPromptCallback } from "./ptyOutputParser";
import type { SessionEvent, SessionEventType } from "../types/insights";

const FLUSH_INTERVAL_MS = 30_000;
const MAX_BUFFER_SIZE = 200;

let eventBuffer: Array<{
  session_id: number;
  event_type: string;
  payload: string;
  created_at: number;
}> = [];

let flushTimer: ReturnType<typeof setInterval> | null = null;

type EventListener = (event: SessionEvent) => void;
const listeners: EventListener[] = [];

export function addEventListener(cb: EventListener): () => void {
  listeners.push(cb);
  return () => {
    const idx = listeners.indexOf(cb);
    if (idx >= 0) listeners.splice(idx, 1);
  };
}

function emit(event: SessionEvent): void {
  for (const cb of listeners) {
    cb(event);
  }

  eventBuffer.push({
    session_id: event.sessionId,
    event_type: event.eventType,
    payload: JSON.stringify(event.payload),
    created_at: event.createdAt,
  });

  if (eventBuffer.length >= MAX_BUFFER_SIZE) {
    flushEvents();
  }
}

async function flushEvents(): Promise<void> {
  if (eventBuffer.length === 0) return;

  const batch = eventBuffer.splice(0);
  try {
    await invoke("record_session_events", { events: batch });
  } catch (e) {
    console.error("[eventCapture] flush failed:", e);
    eventBuffer.unshift(...batch);
  }
}

export function recordEvent(
  sessionId: number,
  eventType: SessionEventType,
  payload: Record<string, unknown>,
): void {
  emit({
    sessionId,
    eventType,
    payload,
    createdAt: Date.now(),
  });
}

export function initEventCapture(): void {
  setPromptCallback((sessionId, text, position) => {
    recordEvent(sessionId, "user_input", { text, position });
  });

  if (flushTimer) clearInterval(flushTimer);
  flushTimer = setInterval(flushEvents, FLUSH_INTERVAL_MS);

  window.addEventListener("beforeunload", () => {
    flushEvents();
  });
}

export function stopEventCapture(): void {
  if (flushTimer) {
    clearInterval(flushTimer);
    flushTimer = null;
  }
  flushEvents();
}
