import { useEffect, useRef, useState, useCallback } from "react";
import { useSessionStore } from "../../stores/sessionStore";
import { useFileViewerStore } from "../../stores/fileViewerStore";
import { useShallow } from "zustand/react/shallow";
import { usePtyBridge } from "../../hooks/usePtyBridge";
import type { Terminal as XTermType } from "@xterm/xterm";

export function Terminal() {
  const terminalRef = useRef<HTMLDivElement>(null);
  const [term, setTerm] = useState<XTermType | null>(null);
  const activeSession = useSessionStore(useShallow((s) => s.getActiveSession()));
  const sessionId = activeSession?.session.id ?? null;

  // Initialize xterm.js instance
  useEffect(() => {
    console.log("[Terminal] init effect, ref exists:", !!terminalRef.current, "sessionId:", sessionId);
    if (!terminalRef.current) return;

    const el = terminalRef.current;
    let xterm: XTermType | null = null;
    let disposed = false;

    const init = async () => {
      const { Terminal: XTerm } = await import("@xterm/xterm");
      const { FitAddon } = await import("@xterm/addon-fit");
      const { WebLinksAddon } = await import("@xterm/addon-web-links");
      const { open } = await import("@tauri-apps/plugin-shell");

      if (disposed) return;

      xterm = new XTerm({
        cursorBlink: true,
        fontSize: 13,
        fontFamily: '"JetBrains Mono", "Fira Code", monospace',
        theme: {
          background: "#1a1a1f",
          foreground: "#d4d4d8",
          cursor: "#6366f1",
          selectionBackground: "#6366f140",
        },
        allowProposedApi: true,
      });

      const fitAddon = new FitAddon();
      xterm.loadAddon(fitAddon);
      xterm.loadAddon(new WebLinksAddon((_e, uri) => {
        open(uri);
      }));
      xterm.open(el);
      fitAddon.fit();

      // File path link provider for Cmd+Click / Ctrl+Click
      xterm.registerLinkProvider({
        provideLinks(bufferLineNumber, callback) {
          const line = xterm!.buffer.active.getLine(bufferLineNumber);
          if (!line) return callback(undefined);

          const text = line.translateToString();
          // Match common file paths (relative paths with extensions, optionally with :lineNumber)
          const pathRegex = /(?:^|\s)((?:[\w.-]+\/)*[\w.-]+\.\w+)(?::(\d+))?/g;
          const links: Array<{
            range: { start: { x: number; y: number }; end: { x: number; y: number } };
            text: string;
            activate: (e: MouseEvent, text: string) => void;
          }> = [];

          let match;
          while ((match = pathRegex.exec(text)) !== null) {
            const fullMatch = match[0].trimStart();
            const filePath = match[1];
            const lineNum = match[2] ? parseInt(match[2], 10) : undefined;
            const startX = text.indexOf(fullMatch, match.index) + 1; // 1-based

            links.push({
              range: {
                start: { x: startX, y: bufferLineNumber + 1 },
                end: { x: startX + fullMatch.length, y: bufferLineNumber + 1 },
              },
              text: fullMatch,
              activate: (_e: MouseEvent) => {
                const { openFile } = useFileViewerStore.getState();
                const activeSession = useSessionStore.getState().getActiveSession();
                if (activeSession) {
                  openFile({
                    sessionId: activeSession.session.id,
                    repoId: activeSession.repo.id,
                    filePath,
                    scrollToLine: lineNum,
                  });
                }
              },
            });
          }

          callback(links.length > 0 ? links : undefined);
        },
      });

      // Debounce fitAddon.fit() to prevent rapid resize events
      // from resetting the terminal scroll position (especially in Tauri WebView)
      let rafId: number | null = null;
      let lastCols = xterm.cols;
      let lastRows = xterm.rows;

      const resizeObserver = new ResizeObserver(() => {
        if (disposed || !xterm) return;
        if (rafId !== null) cancelAnimationFrame(rafId);
        rafId = requestAnimationFrame(() => {
          rafId = null;
          if (disposed || !xterm) return;
          fitAddon.fit();
          // Only apply resize if dimensions actually changed
          if (xterm.cols !== lastCols || xterm.rows !== lastRows) {
            lastCols = xterm.cols;
            lastRows = xterm.rows;
          }
        });
      });
      resizeObserver.observe(el);

      setTerm(xterm);
      console.log("[Terminal] xterm initialized, cols:", xterm.cols, "rows:", xterm.rows);

      return () => {
        resizeObserver.disconnect();
        if (rafId !== null) cancelAnimationFrame(rafId);
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

  // Wire up the PTY bridge
  usePtyBridge({
    sessionId,
    terminal: term,
  });

  // Focus terminal on click
  const handleClick = useCallback(() => {
    term?.focus();
  }, [term]);

  return (
    <div className="relative flex-1" style={{ minHeight: 0 }}>
      {!sessionId && (
        <div className="absolute inset-0 flex items-center justify-center text-zinc-500 z-10">
          <div className="text-center">
            <p className="text-lg font-medium">No active session</p>
            <p className="mt-1 text-sm">
              Create a new session from the sidebar to get started.
            </p>
          </div>
        </div>
      )}
      <div
        ref={terminalRef}
        onClick={handleClick}
        className="h-full overflow-hidden bg-surface-1 p-1"
        style={{ visibility: sessionId ? "visible" : "hidden" }}
      />
    </div>
  );
}
