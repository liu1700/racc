import { useEffect, useRef } from "react";
import { useSessionStore } from "../../stores/sessionStore";

export function Terminal() {
  const terminalRef = useRef<HTMLDivElement>(null);
  const activeSessionId = useSessionStore((s) => s.activeSessionId);

  useEffect(() => {
    if (!terminalRef.current || !activeSessionId) return;

    // xterm.js will be initialized here once we wire up the PTY bridge
    // For now, show a placeholder that indicates the terminal area
    const el = terminalRef.current;
    el.innerHTML = "";

    const initTerminal = async () => {
      const { Terminal: XTerm } = await import("@xterm/xterm");
      const { FitAddon } = await import("@xterm/addon-fit");
      // xterm CSS is imported in index.css or loaded by the addon

      const term = new XTerm({
        cursorBlink: true,
        fontSize: 13,
        fontFamily: '"JetBrains Mono", "Fira Code", monospace',
        theme: {
          background: "#111113",
          foreground: "#e4e4e7",
          cursor: "#6366f1",
          selectionBackground: "#6366f140",
        },
      });

      const fitAddon = new FitAddon();
      term.loadAddon(fitAddon);
      term.open(el);
      fitAddon.fit();

      term.writeln(`\x1b[1;36mOTTE\x1b[0m — Session: ${activeSessionId}`);
      term.writeln("Terminal bridge will connect to tmux session here.");
      term.writeln("");

      const resizeObserver = new ResizeObserver(() => fitAddon.fit());
      resizeObserver.observe(el);

      return () => {
        resizeObserver.disconnect();
        term.dispose();
      };
    };

    const cleanup = initTerminal();
    return () => {
      cleanup.then((fn) => fn?.());
    };
  }, [activeSessionId]);

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
      className="flex-1 bg-surface-1 p-1"
      style={{ minHeight: 0 }}
    />
  );
}
