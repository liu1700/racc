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

    const disposable = terminal.onData((data: string) => {
      writePty(sessionId, data);
    });

    return () => disposable.dispose();
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
