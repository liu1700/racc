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
      // Scroll to bottom after all buffered data is processed
      terminal.write('', () => terminal.scrollToBottom());
      prevSessionRef.current = sessionId;
    }

    // Subscribe to live output
    const unsub = subscribe(sessionId, (data) => {
      // Capture scroll state before write — xterm.js / Tauri WebView reflow
      // can reset viewport to top when processing TUI escape sequences
      const buf = terminal.buffer.active;
      const isAtBottom = buf.baseY === 0 || buf.viewportY >= buf.baseY;
      const savedViewportY = buf.viewportY;

      terminal.write(data, () => {
        if (isAtBottom) {
          terminal.scrollToBottom();
        } else {
          terminal.scrollToLine(savedViewportY);
        }
      });
    });

    return () => {
      unsub?.();
    };
  }, [sessionId, terminal]);

  // Forward keyboard input to PTY
  useEffect(() => {
    if (sessionId === null || !terminal) return;

    terminal.attachCustomKeyEventHandler((event) => {
      // Block Shift+Enter across ALL event types (keydown, keypress, keyup).
      // Returning false only prevents xterm.js processing, but the browser
      // still fires keypress after keydown — xterm.js _keyPress() would then
      // read charCode 13 and send \r to the PTY, causing Claude Code to
      // submit instead of inserting a newline. preventDefault() on keydown
      // stops the keypress from firing; returning false for keypress/keyup
      // is a safety net for WebView edge cases.
      if (event.shiftKey && event.key === 'Enter' && !event.ctrlKey && !event.metaKey && !event.altKey) {
        if (event.type === 'keydown') {
          event.preventDefault();
          writePty(sessionId, '\x1b[13;2u');
        }
        return false;
      }

      // Bypass IME for Shift+punctuation keys so characters like "?" work
      // with Chinese input methods active (IME consumes Shift for mode switching)
      if (event.type !== 'keydown' || event.ctrlKey || event.metaKey || event.altKey) {
        return true;
      }

      if (event.shiftKey && event.key.length === 1) {
        event.preventDefault();
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
