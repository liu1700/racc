import { useEffect, useRef, useState, useCallback } from "react";
import { useSessionStore } from "../../stores/sessionStore";
import { useTmuxBridge } from "../../hooks/useTmuxBridge";
import type { Terminal as XTermType } from "@xterm/xterm";

export function Terminal() {
  const terminalRef = useRef<HTMLDivElement>(null);
  const [term, setTerm] = useState<XTermType | null>(null);
  const activeSessionId = useSessionStore((s) => s.activeSessionId);

  // Initialize xterm.js instance
  useEffect(() => {
    if (!terminalRef.current) return;

    const el = terminalRef.current;
    let xterm: XTermType | null = null;
    let disposed = false;

    const init = async () => {
      const { Terminal: XTerm } = await import("@xterm/xterm");
      const { FitAddon } = await import("@xterm/addon-fit");

      if (disposed) return;

      xterm = new XTerm({
        cursorBlink: true,
        fontSize: 13,
        fontFamily: '"JetBrains Mono", "Fira Code", monospace',
        theme: {
          background: "#111113",
          foreground: "#e4e4e7",
          cursor: "#6366f1",
          selectionBackground: "#6366f140",
        },
        allowProposedApi: true,
      });

      const fitAddon = new FitAddon();
      xterm.loadAddon(fitAddon);
      xterm.open(el);
      fitAddon.fit();

      const resizeObserver = new ResizeObserver(() => {
        if (!disposed) fitAddon.fit();
      });
      resizeObserver.observe(el);

      setTerm(xterm);

      return () => {
        resizeObserver.disconnect();
      };
    };

    const cleanupPromise = init();

    return () => {
      disposed = true;
      cleanupPromise.then((cleanup) => cleanup?.());
      if (xterm) {
        xterm.dispose();
        setTerm(null);
      }
    };
  }, []);

  // Reset terminal content when switching sessions
  const prevSessionRef = useRef<string | null>(null);
  useEffect(() => {
    if (activeSessionId !== prevSessionRef.current && term) {
      term.reset();
      prevSessionRef.current = activeSessionId;
    }
  }, [activeSessionId, term]);

  // Wire up the tmux bridge
  useTmuxBridge({
    sessionId: activeSessionId,
    terminal: term,
  });

  // Focus terminal on click
  const handleClick = useCallback(() => {
    term?.focus();
  }, [term]);

  if (!activeSessionId) {
    return (
      <div className="flex flex-1 items-center justify-center text-zinc-500">
        <div className="text-center">
          <p className="text-lg font-medium">No active session</p>
          <p className="mt-1 text-sm">
            Create a new session from the sidebar to get started.
          </p>
        </div>
      </div>
    );
  }

  return (
    <div
      ref={terminalRef}
      onClick={handleClick}
      className="flex-1 bg-surface-1 p-1"
      style={{ minHeight: 0 }}
    />
  );
}
