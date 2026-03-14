import { useEffect, useRef, useState, useCallback } from "react";
import { useFileViewerStore } from "../../stores/fileViewerStore";
import { codeToHtml } from "shiki";
import { useSessionStore } from "../../stores/sessionStore";
import { useShallow } from "zustand/react/shallow";
import "./fileViewer.css";

export function FileViewer() {
  const { isOpen, content, loading, error, filePath, scrollToLine, highlightRange: _highlightRange, closeViewer } =
    useFileViewerStore();
  const activeSession = useSessionStore(
    useShallow((s) => s.getActiveSession()),
  );

  const codeRef = useRef<HTMLDivElement>(null);
  const [highlightedHtml, setHighlightedHtml] = useState<string>("");
  const [isVisible, setIsVisible] = useState(false);

  // Search state
  const [searchOpen, setSearchOpen] = useState(false);
  const [searchQuery, setSearchQuery] = useState("");
  const [matchIndex, setMatchIndex] = useState(0);
  const [matchCount, setMatchCount] = useState(0);
  const searchInputRef = useRef<HTMLInputElement>(null);

  // Jump to line state
  const [jumpOpen, setJumpOpen] = useState(false);
  const [jumpValue, setJumpValue] = useState("");
  const jumpInputRef = useRef<HTMLInputElement>(null);

  // Animate in/out
  useEffect(() => {
    if (isOpen) {
      requestAnimationFrame(() => setIsVisible(true));
    } else {
      setIsVisible(false);
    }
  }, [isOpen]);

  // Syntax highlight with Shiki
  useEffect(() => {
    if (!content) return;

    let cancelled = false;
    codeToHtml(content.content, {
      lang: content.language,
      theme: "github-dark-default",
    })
      .then((html) => {
        if (!cancelled) setHighlightedHtml(html);
      })
      .catch(() => {
        // Fallback: plain text with line numbers
        if (!cancelled) {
          const lines = content.content.split("\n");
          const escaped = lines
            .map(
              (line, i) =>
                `<span class="line-number">${i + 1}</span>${escapeHtml(line)}`,
            )
            .join("\n");
          setHighlightedHtml(`<pre><code>${escaped}</code></pre>`);
        }
      });

    return () => {
      cancelled = true;
    };
  }, [content]);

  // Scroll to target line after rendering
  useEffect(() => {
    if (!highlightedHtml || !scrollToLine || !codeRef.current) return;

    const timer = setTimeout(() => {
      const lineEl = codeRef.current?.querySelector(
        `[data-line="${scrollToLine}"]`,
      );
      if (lineEl) {
        lineEl.scrollIntoView({ block: "center", behavior: "smooth" });
      } else {
        // Fallback: estimate scroll position based on line height
        const lineHeight = 20;
        codeRef.current?.scrollTo({
          top: (scrollToLine - 1) * lineHeight,
          behavior: "smooth",
        });
      }
    }, 50);

    return () => clearTimeout(timer);
  }, [highlightedHtml, scrollToLine]);

  // Keyboard shortcuts
  useEffect(() => {
    if (!isOpen) return;

    const handler = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.preventDefault();
        if (jumpOpen) {
          setJumpOpen(false);
        } else if (searchOpen) {
          setSearchOpen(false);
          setSearchQuery("");
        } else {
          closeViewer();
        }
        return;
      }

      // Cmd+F / Ctrl+F — open search
      if ((e.metaKey || e.ctrlKey) && e.key === "f") {
        e.preventDefault();
        setSearchOpen(true);
        setTimeout(() => searchInputRef.current?.focus(), 0);
        return;
      }

      // Ctrl+G — jump to line
      if (e.ctrlKey && e.key === "g") {
        e.preventDefault();
        setJumpOpen(true);
        setTimeout(() => jumpInputRef.current?.focus(), 0);
        return;
      }
    };

    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [isOpen, searchOpen, jumpOpen, closeViewer]);

  // Search logic
  const handleSearch = useCallback(
    (query: string) => {
      setSearchQuery(query);
      if (!codeRef.current || !query) {
        setMatchCount(0);
        setMatchIndex(0);
        return;
      }

      // Remove previous highlights
      codeRef.current
        .querySelectorAll(".search-highlight")
        .forEach((el) => {
          const parent = el.parentNode;
          if (parent) {
            parent.replaceChild(document.createTextNode(el.textContent || ""), el);
            parent.normalize();
          }
        });

      // Find and highlight matches in text nodes
      const walker = document.createTreeWalker(
        codeRef.current,
        NodeFilter.SHOW_TEXT,
      );
      const matches: Element[] = [];
      const lowerQuery = query.toLowerCase();
      const nodesToProcess: { node: Text; indices: number[] }[] = [];

      let node: Text | null;
      while ((node = walker.nextNode() as Text | null)) {
        const text = node.textContent || "";
        const lower = text.toLowerCase();
        const indices: number[] = [];
        let idx = lower.indexOf(lowerQuery);
        while (idx !== -1) {
          indices.push(idx);
          idx = lower.indexOf(lowerQuery, idx + 1);
        }
        if (indices.length > 0) {
          nodesToProcess.push({ node, indices });
        }
      }

      for (const { node: textNode, indices } of nodesToProcess) {
        const text = textNode.textContent || "";
        const parent = textNode.parentNode;
        if (!parent) continue;

        const frag = document.createDocumentFragment();
        let lastIdx = 0;

        for (const idx of indices) {
          if (idx > lastIdx) {
            frag.appendChild(document.createTextNode(text.slice(lastIdx, idx)));
          }
          const span = document.createElement("span");
          span.className = "search-highlight";
          span.textContent = text.slice(idx, idx + query.length);
          frag.appendChild(span);
          matches.push(span);
          lastIdx = idx + query.length;
        }

        if (lastIdx < text.length) {
          frag.appendChild(document.createTextNode(text.slice(lastIdx)));
        }

        parent.replaceChild(frag, textNode);
      }

      setMatchCount(matches.length);
      if (matches.length > 0) {
        setMatchIndex(0);
        matches[0].classList.add("search-current");
        matches[0].scrollIntoView({ block: "center" });
      }
    },
    [],
  );

  const navigateMatch = useCallback(
    (direction: 1 | -1) => {
      if (!codeRef.current || matchCount === 0) return;
      const highlights = codeRef.current.querySelectorAll(".search-highlight");
      highlights[matchIndex]?.classList.remove("search-current");
      const next = (matchIndex + direction + matchCount) % matchCount;
      setMatchIndex(next);
      highlights[next]?.classList.add("search-current");
      highlights[next]?.scrollIntoView({ block: "center" });
    },
    [matchIndex, matchCount],
  );

  // Jump to line
  const handleJump = useCallback(
    (lineStr: string) => {
      const line = parseInt(lineStr, 10);
      if (isNaN(line) || !codeRef.current) return;

      const lineHeight = 20;
      codeRef.current.scrollTo({
        top: (line - 1) * lineHeight,
        behavior: "smooth",
      });
      setJumpOpen(false);
      setJumpValue("");
    },
    [],
  );

  // Click to highlight line
  const handleCodeClick = useCallback((e: React.MouseEvent) => {
    const target = e.target as HTMLElement;
    const lineEl = target.closest("[data-line]") || target.closest(".line");
    if (!lineEl) return;

    // Remove previous active
    codeRef.current
      ?.querySelectorAll(".line-active")
      .forEach((el) => el.classList.remove("line-active"));
    lineEl.classList.add("line-active");
  }, []);

  if (!isOpen) return null;

  return (
    <div
      className={`absolute inset-0 z-30 flex flex-col bg-surface-0/95 transition-opacity duration-150 ${isVisible ? "opacity-100" : "opacity-0"}`}
    >
      {/* Top bar */}
      <div className="flex items-center justify-between border-b border-surface-3 px-4 py-2">
        <div className="flex items-center gap-3 text-sm">
          <span className="text-zinc-300">{filePath}</span>
          {content && (
            <span className="text-zinc-500">
              {content.total_lines} lines · {content.language} · {content.encoding}
              {content.is_truncated && ` · showing first ${content.line_count}`}
            </span>
          )}
        </div>
        <div className="flex items-center gap-2 text-xs text-zinc-500">
          <kbd className="rounded bg-surface-2 px-1.5 py-0.5">Cmd+F</kbd>
          <kbd className="rounded bg-surface-2 px-1.5 py-0.5">Esc</kbd>
          <button
            onClick={closeViewer}
            className="ml-2 rounded p-1 text-zinc-400 hover:bg-surface-2 hover:text-zinc-200"
          >
            x
          </button>
        </div>
      </div>

      {/* Search bar */}
      {searchOpen && (
        <div className="flex items-center gap-2 border-b border-surface-3 px-4 py-1.5">
          <input
            ref={searchInputRef}
            type="text"
            value={searchQuery}
            onChange={(e) => handleSearch(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter" && !e.nativeEvent.isComposing) {
                e.preventDefault();
                navigateMatch(e.shiftKey ? -1 : 1);
              }
              if (e.key === "Escape") {
                e.preventDefault();
                setSearchOpen(false);
                setSearchQuery("");
              }
            }}
            placeholder="Search in file..."
            className="w-64 rounded bg-surface-2 px-2 py-1 text-sm text-zinc-200 outline-none focus:ring-1 focus:ring-accent"
          />
          {matchCount > 0 && (
            <>
              <span className="text-xs text-zinc-400">
                {matchIndex + 1}/{matchCount}
              </span>
              <button onClick={() => navigateMatch(-1)} className="text-zinc-400 hover:text-zinc-200">
                ↑
              </button>
              <button onClick={() => navigateMatch(1)} className="text-zinc-400 hover:text-zinc-200">
                ↓
              </button>
            </>
          )}
        </div>
      )}

      {/* Jump to line */}
      {jumpOpen && (
        <div className="flex items-center gap-2 border-b border-surface-3 px-4 py-1.5">
          <span className="text-xs text-zinc-400">Go to line:</span>
          <input
            ref={jumpInputRef}
            type="text"
            value={jumpValue}
            onChange={(e) => setJumpValue(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter" && !e.nativeEvent.isComposing) {
                e.preventDefault();
                handleJump(jumpValue);
              }
              if (e.key === "Escape") {
                e.preventDefault();
                setJumpOpen(false);
                setJumpValue("");
              }
            }}
            className="w-20 rounded bg-surface-2 px-2 py-1 text-sm text-zinc-200 outline-none focus:ring-1 focus:ring-accent"
          />
        </div>
      )}

      {/* Code area */}
      <div
        ref={codeRef}
        className="flex-1 overflow-auto"
        onClick={handleCodeClick}
      >
        {loading && (
          <div className="flex h-full items-center justify-center text-zinc-500">
            Loading...
          </div>
        )}
        {error && (
          <div className="flex h-full flex-col items-center justify-center gap-2 text-zinc-400">
            <p>{error}</p>
            <button
              onClick={closeViewer}
              className="rounded bg-surface-2 px-3 py-1 text-sm hover:bg-surface-3"
            >
              Dismiss
            </button>
          </div>
        )}
        {!loading && !error && highlightedHtml && (
          <div
            className="file-viewer-code p-4 text-[13px] leading-5"
            style={{ fontFamily: '"JetBrains Mono", "Fira Code", monospace' }}
            dangerouslySetInnerHTML={{ __html: highlightedHtml }}
          />
        )}
      </div>

      {/* Bottom status strip */}
      {activeSession && (
        <div className="border-t border-surface-3 px-4 py-1 text-xs text-zinc-500">
          <span className="mr-2">▪</span>
          <span>{activeSession.session.branch || "main"}</span>
          <span className="mx-1">·</span>
          <span>{activeSession.session.status}</span>
          <span className="mx-1">·</span>
          <span>{formatElapsed(activeSession.session.created_at)}</span>
        </div>
      )}
    </div>
  );
}

function formatElapsed(createdAt: string): string {
  const elapsed = Date.now() - new Date(createdAt).getTime();
  const minutes = Math.floor(elapsed / 60_000);
  if (minutes < 1) return "<1m";
  if (minutes < 60) return `${minutes}m`;
  const hours = Math.floor(minutes / 60);
  const mins = minutes % 60;
  if (mins === 0) return `${hours}h`;
  return `${hours}h ${mins}m`;
}

function escapeHtml(text: string): string {
  return text
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;");
}
