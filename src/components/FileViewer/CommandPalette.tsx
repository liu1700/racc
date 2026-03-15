import { useEffect, useRef, useState } from "react";
import { useFileViewerStore } from "../../stores/fileViewerStore";
import { useSessionStore } from "../../stores/sessionStore";
import { useShallow } from "zustand/react/shallow";
import { useIMEComposition } from "../../hooks/useIMEComposition";

export function CommandPalette() {
  const { isPaletteOpen, searchResults, searchLoading, closePalette, searchFiles, openFile } =
    useFileViewerStore();
  const activeSession = useSessionStore(useShallow((s) => s.getActiveSession()));
  const repos = useSessionStore((s) => s.repos);

  const inputRef = useRef<HTMLInputElement>(null);
  const [query, setQuery] = useState("");
  const [selectedIndex, setSelectedIndex] = useState(0);
  const { isComposingRef, compositionProps } = useIMEComposition();

  // Resolve session/repo context
  const sessionId = activeSession?.session.id ?? null;
  const repoId = activeSession?.repo.id ?? (repos.length === 1 ? repos[0].repo.id : null);
  const hasContext = sessionId !== null || repoId !== null;

  // Focus input on open
  useEffect(() => {
    if (isPaletteOpen) {
      setQuery("");
      setSelectedIndex(0);
      setTimeout(() => inputRef.current?.focus(), 0);
    }
  }, [isPaletteOpen]);

  // Debounced search
  useEffect(() => {
    if (!isPaletteOpen || !hasContext) return;

    const timer = setTimeout(() => {
      searchFiles({ sessionId, repoId, query });
    }, 100);

    return () => clearTimeout(timer);
  }, [query, isPaletteOpen, sessionId, repoId, hasContext, searchFiles]);

  // Reset selection when results change
  useEffect(() => {
    setSelectedIndex(0);
  }, [searchResults]);

  const selectFile = (filePath: string) => {
    openFile({ sessionId, repoId, filePath });
  };

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === "Escape") {
      e.preventDefault();
      closePalette();
      return;
    }

    if (e.key === "ArrowDown") {
      e.preventDefault();
      setSelectedIndex((i) => Math.min(i + 1, searchResults.length - 1));
      return;
    }

    if (e.key === "ArrowUp") {
      e.preventDefault();
      setSelectedIndex((i) => Math.max(i - 1, 0));
      return;
    }

    if (e.key === "Enter" && !isComposingRef.current && searchResults.length > 0) {
      e.preventDefault();
      selectFile(searchResults[selectedIndex].relative_path);
      return;
    }
  };

  if (!isPaletteOpen) return null;

  return (
    <div className="fixed inset-0 z-40 flex items-start justify-center pt-[15%]">
      {/* Backdrop */}
      <div className="fixed inset-0 bg-black/50" onClick={closePalette} />

      {/* Palette */}
      <div className="relative w-full max-w-lg rounded-lg border border-surface-3 bg-surface-1 shadow-2xl">
        {/* Input */}
        <div className="flex items-center border-b border-surface-3 px-3">
          <span className="text-zinc-500 text-sm">&gt;</span>
          <input
            ref={inputRef}
            type="text"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            onKeyDown={handleKeyDown}
            {...compositionProps}
            placeholder={hasContext ? "Search files..." : "Select a repo first"}
            disabled={!hasContext}
            className="w-full bg-transparent px-2 py-2.5 text-sm text-zinc-200 outline-none placeholder:text-zinc-600"
          />
          {searchLoading && (
            <span className="text-xs text-zinc-500">...</span>
          )}
        </div>

        {/* Results */}
        {hasContext && searchResults.length > 0 && (
          <div className="max-h-64 overflow-auto py-1">
            {searchResults.map((result, i) => (
              <button
                key={result.relative_path}
                onClick={() => selectFile(result.relative_path)}
                className={`flex w-full items-center px-3 py-1.5 text-left text-sm ${
                  i === selectedIndex
                    ? "bg-accent/20 text-zinc-100"
                    : "text-zinc-400 hover:bg-surface-2 hover:text-zinc-200"
                }`}
              >
                <span className="truncate">{result.relative_path}</span>
              </button>
            ))}
          </div>
        )}

        {/* No results */}
        {hasContext && query && !searchLoading && searchResults.length === 0 && (
          <div className="px-3 py-4 text-center text-sm text-zinc-500">
            No files found
          </div>
        )}

        {/* No context */}
        {!hasContext && (
          <div className="px-3 py-4 text-center text-sm text-zinc-500">
            Select a repo first to search files
          </div>
        )}
      </div>
    </div>
  );
}
