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

    // IMPORTANT: Must use attachCustomKeyEventHandler (fires BEFORE xterm processes
    // the key) not onKey (fires AFTER). Using onKey would double-send the keystroke.
    terminal.attachCustomKeyEventHandler((e) => {
      if (e.type !== "keydown" || e.ctrlKey || e.metaKey || e.altKey) {
        return true;
      }

      if (e.shiftKey) {
        // Shift+Enter: send kitty keyboard protocol sequence so TUI apps
        // (Claude Code, etc.) receive it as a distinct key from plain Enter
        if (e.key === "Enter") {
          e.preventDefault();
          ptyManager.write(sessionId, "\x1b[13;2u");
          return false;
        }

        // Shift+single-char: bypass IME mode-switching
        if (e.key.length === 1) {
          return false;
        }
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
