import { useEffect, useRef } from "react";
import type { Terminal } from "@xterm/xterm";
import { subscribe, getBuffer, writePty, resizePty } from "../services/ptyManager";

interface UsePtyBridgeOptions {
  sessionId: number | null;
  terminal: Terminal | null;
}

export function usePtyBridge({ sessionId, terminal }: UsePtyBridgeOptions) {
  const prevSessionRef = useRef<number | null>(null);

  // Connect PTY output to xterm
  useEffect(() => {
    console.log("[usePtyBridge] output effect:", { sessionId, hasTerminal: !!terminal });
    if (sessionId === null || !terminal) return;

    // On session switch: clear terminal and replay buffer
    if (sessionId !== prevSessionRef.current) {
      terminal.reset();
      const buffer = getBuffer(sessionId);
      for (const chunk of buffer) {
        terminal.write(chunk);
      }
      prevSessionRef.current = sessionId;
    }

    // Subscribe to live output
    const unsub = subscribe(sessionId, (data) => {
      terminal.write(data);
    });

    return () => {
      unsub?.();
    };
  }, [sessionId, terminal]);

  // Forward keyboard input to PTY
  useEffect(() => {
    if (sessionId === null || !terminal) return;

    // Bypass IME for Shift+punctuation keys so characters like "?" work
    // with Chinese input methods active (IME consumes Shift for mode switching)
    terminal.attachCustomKeyEventHandler((event) => {
      if (
        event.type === 'keydown' &&
        event.shiftKey &&
        !event.ctrlKey &&
        !event.metaKey &&
        !event.altKey &&
        event.key.length === 1
      ) {
        writePty(sessionId, event.key);
        return false;
      }
      return true;
    });

    const disposable = terminal.onData((data: string) => {
      writePty(sessionId, data);
    });

    return () => {
      terminal.attachCustomKeyEventHandler(() => true);
      disposable.dispose();
    };
  }, [sessionId, terminal]);

  // Sync terminal size to PTY
  useEffect(() => {
    if (sessionId === null || !terminal) return;

    resizePty(sessionId, terminal.cols, terminal.rows);

    const disposable = terminal.onResize(({ cols, rows }) => {
      resizePty(sessionId, cols, rows);
    });

    return () => disposable.dispose();
  }, [sessionId, terminal]);
}
