import { useState, useRef, useEffect } from "react";
import { open as openShell } from "@tauri-apps/plugin-shell";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { useSessionStore } from "../../stores/sessionStore";
import { RemoveSessionDialog } from "./RemoveSessionDialog";

import type { Session, SessionStatus } from "../../types/session";
import { parsePrDisplay } from "../../utils/prUrl";

const statusColor: Record<SessionStatus, string> = {
  Running: "bg-status-running",
  Completed: "bg-status-completed",
  Disconnected: "bg-status-disconnected",
  Error: "bg-status-error",
};

// Sort priority: errors/blocked first, then running, then completed/disconnected
const statusPriority: Record<SessionStatus, number> = {
  Error: 0,
  Disconnected: 1,
  Running: 2,
  Completed: 3,
};

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

function sortByStatus(sessions: Session[]): Session[] {
  return [...sessions].sort(
    (a, b) => statusPriority[a.status] - statusPriority[b.status],
  );
}

interface SidebarProps {
  onNewTask?: () => void;
  onSessionSelect?: () => void;
}

export function Sidebar({ onNewTask, onSessionSelect }: SidebarProps) {
  const repos = useSessionStore((s) => s.repos);
  const activeSessionId = useSessionStore((s) => s.activeSessionId);
  const setActiveSession = useSessionStore((s) => s.setActiveSession);
  const reattachSession = useSessionStore((s) => s.reattachSession);
  const removeRepo = useSessionStore((s) => s.removeRepo);

  const importRepo = useSessionStore((s) => s.importRepo);
  const [expandedRepos, setExpandedRepos] = useState<Set<number>>(new Set());
  const sessionLastOutput = useSessionStore((s) => s.sessionLastOutput);
  const [removeDialogSession, setRemoveDialogSession] = useState<Session | null>(null);
const [repoDropdownOpen, setRepoDropdownOpen] = useState(false);
  const dropdownRef = useRef<HTMLDivElement>(null);

  const activeRepo = repos.find((r) =>
    r.sessions.some((s) => s.id === activeSessionId)
  )?.repo ?? repos[0]?.repo ?? null;

  const handleImportRepo = async () => {
    const selected = await openDialog({ directory: true, multiple: false });
    if (selected) await importRepo(selected);
    setRepoDropdownOpen(false);
  };

  useEffect(() => {
    if (!repoDropdownOpen) return;
    const handler = (e: MouseEvent) => {
      if (dropdownRef.current && !dropdownRef.current.contains(e.target as Node)) {
        setRepoDropdownOpen(false);
      }
    };
    document.addEventListener("mousedown", handler);
    return () => document.removeEventListener("mousedown", handler);
  }, [repoDropdownOpen]);

  const toggleRepo = (repoId: number) => {
    setExpandedRepos((prev) => {
      const next = new Set(prev);
      if (next.has(repoId)) next.delete(repoId);
      else next.add(repoId);
      return next;
    });
  };

  const isExpanded = (repoId: number) => {
    const rws = repos.find((r) => r.repo.id === repoId);
    return expandedRepos.has(repoId) || (rws?.sessions.length ?? 0) > 0;
  };

  return (
    <aside className="flex w-56 flex-col overflow-y-auto border-r border-surface-3 bg-surface-1">
      <div className="relative border-b border-surface-3" ref={dropdownRef}>
        {activeRepo ? (
          <button
            onClick={() => setRepoDropdownOpen((v) => !v)}
            className="flex w-full items-center gap-2 px-3 py-2 text-left transition-colors hover:bg-surface-2"
          >
            <span className="flex-1 truncate text-xs font-medium text-zinc-300">
              {activeRepo.name}
            </span>
            <span className="text-[10px] text-zinc-500">▾</span>
          </button>
        ) : (
          <button
            onClick={handleImportRepo}
            className="w-full px-3 py-2 text-left text-xs text-zinc-400 transition-colors hover:bg-surface-2 hover:text-zinc-200"
          >
            Select a git repo
          </button>
        )}

        {repoDropdownOpen && (
          <div className="absolute left-0 right-0 top-full z-50 border-b border-surface-3 bg-surface-1 shadow-lg">
            {repos.map(({ repo }) => (
              <button
                key={repo.id}
                onClick={() => {
                  toggleRepo(repo.id);
                  setRepoDropdownOpen(false);
                }}
                className={`flex w-full items-center gap-2 px-3 py-1.5 text-left text-xs transition-colors hover:bg-surface-2 ${
                  repo.id === activeRepo?.id ? "text-accent" : "text-zinc-400"
                }`}
                title={repo.path}
              >
                {repo.id === activeRepo?.id && <span className="text-[10px]">●</span>}
                <span className="truncate">{repo.name}</span>
              </button>
            ))}
            <div className="border-t border-surface-3">
              <button
                onClick={handleImportRepo}
                className="flex w-full items-center gap-2 px-3 py-1.5 text-left text-xs text-zinc-500 transition-colors hover:bg-surface-2 hover:text-zinc-200"
              >
                <span className="text-base leading-none">+</span>
                Import new repo...
              </button>
            </div>
          </div>
        )}
      </div>

      <div className="flex-1 overflow-y-auto px-1 py-1">

        {repos.map(({ repo, sessions }) => (
          <div key={repo.id} className="mb-1">
            <div className="group flex items-center rounded px-2 py-1.5 transition-colors duration-150 hover:bg-surface-2">
              <button
                onClick={() => toggleRepo(repo.id)}
                className="mr-1 text-xs text-zinc-500"
              >
                {isExpanded(repo.id) ? "▼" : "▶"}
              </button>
              <span
                className="flex-1 cursor-pointer truncate text-xs font-medium text-zinc-300"
                onClick={() => toggleRepo(repo.id)}
                title={repo.path}
              >
                {repo.name}
              </span>
              <button
                onClick={() => onNewTask?.()}
                className="ml-1 hidden rounded px-1 text-xs text-zinc-500 transition-colors duration-150 hover:text-accent group-hover:block"
                title="New task"
              >
                +
              </button>
              <button
                onClick={() => removeRepo(repo.id)}
                className="ml-1 hidden rounded px-1 text-xs text-zinc-500 transition-colors duration-150 hover:text-red-400 group-hover:block"
                title="Remove repo"
              >
                ×
              </button>
            </div>

            {isExpanded(repo.id) &&
              sortByStatus(sessions).map((session) => (
                <div
                  key={session.id}
                  onClick={() => {
                    if (session.status === "Running") {
                      setActiveSession(session.id);
                    } else {
                      reattachSession(session.id);
                    }
                    onSessionSelect?.();
                  }}
                  className={`group ml-4 cursor-pointer rounded px-2 py-1 transition-colors duration-150 ${
                    session.id === activeSessionId
                      ? "bg-surface-3"
                      : "hover:bg-surface-2"
                  }`}
                >
                  <div className="flex items-center gap-2">
                    <span
                      className={`h-1.5 w-1.5 flex-shrink-0 rounded-full ${statusColor[session.status]} ${
                        session.status === "Running" ? "animate-status-pulse" : ""
                      }`}
                    />
                    <span className="flex-1 truncate text-xs text-zinc-400">
                      {session.branch ?? "main"}
                    </span>
                    <span className="text-[10px] tabular-nums text-zinc-600">
                      {formatElapsed(session.created_at)}
                    </span>
                    {session.status !== "Running" && (
                      <button
                        onClick={(e) => {
                          e.stopPropagation();
                          reattachSession(session.id);
                        }}
                        className="hidden text-xs text-zinc-500 transition-colors duration-150 hover:text-accent group-hover:block"
                        title="Reattach session"
                      >
                        ▶
                      </button>
                    )}
                    <button
                      onClick={(e) => {
                        e.stopPropagation();
                        setRemoveDialogSession(session);
                      }}
                      className="hidden text-xs text-zinc-500 transition-colors duration-150 hover:text-red-400 group-hover:block"
                      title="Remove session"
                    >
                      ×
                    </button>
                  </div>
                  {session.status === "Running" && (
                    <p className="mt-0.5 h-3.5 truncate text-[10px] text-zinc-600 leading-tight pl-3.5">
                      {sessionLastOutput[session.id] ?? "\u00A0"}
                    </p>
                  )}
                  {session.pr_url && (() => {
                    const pr = parsePrDisplay(session.pr_url);
                    return pr ? (
                      <button
                        onClick={(e) => {
                          e.stopPropagation();
                          openShell(session.pr_url!);
                        }}
                        className="mt-0.5 flex items-center gap-1 pl-3.5 text-[10px] text-accent hover:underline"
                        title={session.pr_url}
                      >
                        {pr.label}
                      </button>
                    ) : null;
                  })()}
                </div>
              ))}
          </div>
        ))}
      </div>

      {removeDialogSession !== null && (
        <RemoveSessionDialog
          session={removeDialogSession}
          open={true}
          onClose={() => setRemoveDialogSession(null)}
        />
      )}

    </aside>
  );
}
