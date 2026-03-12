import { useEffect, useRef, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { Terminal } from "@xterm/xterm";

interface UseTmuxBridgeOptions {
  sessionId: string | null;
  terminal: Terminal | null;
  pollIntervalMs?: number;
}

export function useTmuxBridge({
  sessionId,
  terminal,
  pollIntervalMs = 150,
}: UseTmuxBridgeOptions) {
  const lastContentRef = useRef<string>("");
  const pollRef = useRef<ReturnType<typeof setInterval> | null>(null);

  // Poll capture_pane and write new content to xterm
  const poll = useCallback(async () => {
    if (!sessionId || !terminal) return;

    try {
      const content = await invoke<string>("capture_pane", {
        sessionId,
      });

      // Only update if content changed
      if (content !== lastContentRef.current) {
        lastContentRef.current = content;
        // Move cursor to home position and clear screen, then write new content.
        // Using ANSI sequences instead of terminal.reset() to avoid flickering.
        terminal.write("\x1b[H\x1b[2J");
        terminal.write(content);
      }
    } catch {
      // Session might have ended — stop polling silently
    }
  }, [sessionId, terminal]);

  // Start/stop polling when session or terminal changes
  useEffect(() => {
    if (!sessionId || !terminal) {
      if (pollRef.current) {
        clearInterval(pollRef.current);
        pollRef.current = null;
      }
      return;
    }

    // Reset content tracking for new session
    lastContentRef.current = "";

    // Initial fetch
    poll();

    // Start polling
    pollRef.current = setInterval(poll, pollIntervalMs);

    return () => {
      if (pollRef.current) {
        clearInterval(pollRef.current);
        pollRef.current = null;
      }
    };
  }, [sessionId, terminal, poll, pollIntervalMs]);

  // Forward keyboard input to tmux
  useEffect(() => {
    if (!sessionId || !terminal) return;

    const disposable = terminal.onData(async (data: string) => {
      try {
        if (data === "\r") {
          await invoke("send_special_key", { sessionId, key: "Enter" });
        } else if (data === "\x03") {
          await invoke("send_special_key", { sessionId, key: "C-c" });
        } else if (data === "\x04") {
          await invoke("send_special_key", { sessionId, key: "C-d" });
        } else if (data === "\x1a") {
          await invoke("send_special_key", { sessionId, key: "C-z" });
        } else if (data === "\x1b") {
          await invoke("send_special_key", { sessionId, key: "Escape" });
        } else if (data === "\x7f" || data === "\b") {
          await invoke("send_special_key", { sessionId, key: "BSpace" });
        } else if (data === "\t") {
          await invoke("send_special_key", { sessionId, key: "Tab" });
        } else if (data.startsWith("\x1b[")) {
          const arrowMap: Record<string, string> = {
            "\x1b[A": "Up",
            "\x1b[B": "Down",
            "\x1b[C": "Right",
            "\x1b[D": "Left",
            "\x1b[H": "Home",
            "\x1b[F": "End",
            "\x1b[3~": "DC",
          };
          const mapped = arrowMap[data];
          if (mapped) {
            await invoke("send_special_key", { sessionId, key: mapped });
          }
        } else {
          await invoke("send_keys", { sessionId, keys: data });
        }
      } catch {
        // Session might have ended
      }
    });

    return () => disposable.dispose();
  }, [sessionId, terminal]);

  // Sync terminal size to tmux pane
  useEffect(() => {
    if (!sessionId || !terminal) return;

    const syncSize = () => {
      invoke("resize_pane", {
        sessionId,
        cols: terminal.cols,
        rows: terminal.rows,
      }).catch(() => {});
    };

    syncSize();

    const disposable = terminal.onResize(syncSize);
    return () => disposable.dispose();
  }, [sessionId, terminal]);
}
