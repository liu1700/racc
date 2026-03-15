import { useEffect } from "react";
import { listen } from "@tauri-apps/api/event";
import { ptyManager } from "../services/ptyManager";
import type { Terminal } from "@xterm/xterm";

interface UsePtyBridgeOptions {
  sessionId: number | null;
  terminal: Terminal | null;
}

export function usePtyBridge({ sessionId, terminal }: UsePtyBridgeOptions) {
  // Output: listen for transport:data events
  useEffect(() => {
    if (!terminal || sessionId == null) return;

    // Replay buffer on session switch
    terminal.reset();
    ptyManager
      .getBuffer(sessionId)
      .then((buffer) => {
        if (buffer.length > 0) {
          terminal.write(buffer);
        }
        // Scroll to bottom after all buffered data is processed
        terminal.write("", () => terminal.scrollToBottom());
      })
      .catch(() => {}); // Buffer may not exist yet

    // Listen for live output
    const unlisten = listen<{ session_id: number; data: number[] }>(
      "transport:data",
      (event) => {
        if (event.payload.session_id === sessionId) {
          terminal.write(new Uint8Array(event.payload.data));
        }
      }
    );

    return () => {
      unlisten.then((fn) => fn());
    };
  }, [sessionId, terminal]);

  // Input: forward keyboard to transport
  useEffect(() => {
    if (!terminal || sessionId == null) return;

    const onData = terminal.onData((data) => {
      ptyManager.write(sessionId, data);
    });

    // Block Shift+Enter across ALL event types (keydown, keypress, keyup).
    // Returning false only prevents xterm.js processing, but the browser
    // still fires keypress after keydown — xterm.js _keyPress() would then
    // read charCode 13 and send \r to the PTY, causing Claude Code to
    // submit instead of inserting a newline. preventDefault() on keydown
    // stops the keypress from firing; returning false for keypress/keyup
    // is a safety net for WebView edge cases.
    terminal.attachCustomKeyEventHandler((event) => {
      if (event.shiftKey && event.key === 'Enter' && !event.ctrlKey && !event.metaKey && !event.altKey) {
        if (event.type === 'keydown') {
          event.preventDefault();
          ptyManager.write(sessionId, '\x1b[13;2u');
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
        ptyManager.write(sessionId, event.key);
        return false;
      }

      return true;
    });

    return () => {
      terminal.attachCustomKeyEventHandler(() => true);
      onData.dispose();
    };
  }, [sessionId, terminal]);

  // Resize: sync terminal dimensions to transport
  useEffect(() => {
    if (!terminal || sessionId == null) return;

    // Send initial size
    ptyManager.resize(sessionId, terminal.cols, terminal.rows);

    const disposable = terminal.onResize(({ cols, rows }) => {
      ptyManager.resize(sessionId, cols, rows);
    });

    return () => disposable.dispose();
  }, [sessionId, terminal]);
}
