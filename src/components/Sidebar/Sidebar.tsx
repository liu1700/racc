import { useState } from "react";
import { open } from "@tauri-apps/plugin-shell";
import { useSessionStore } from "../../stores/sessionStore";
import { ImportRepoDialog } from "./ImportRepoDialog";
import { NewAgentDialog } from "./NewAgentDialog";
import { RemoveSessionDialog } from "./RemoveSessionDialog";
import { ResetDbDialog } from "./ResetDbDialog";
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

export function Sidebar() {
  const repos = useSessionStore((s) => s.repos);
  const activeSessionId = useSessionStore((s) => s.activeSessionId);
  const setActiveSession = useSessionStore((s) => s.setActiveSession);
  const stopSession = useSessionStore((s) => s.stopSession);
  const reattachSession = useSessionStore((s) => s.reattachSession);
  const removeRepo = useSessionStore((s) => s.removeRepo);

  const [expandedRepos, setExpandedRepos] = useState<Set<number>>(new Set());
  const sessionLastOutput = useSessionStore((s) => s.sessionLastOutput);
  const [agentDialogRepoId, setAgentDialogRepoId] = useState<number | null>(null);
  const [removeDialogSession, setRemoveDialogSession] = useState<Session | null>(null);
  const [resetDialogOpen, setResetDialogOpen] = useState(false);

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
      <div className="border-b border-surface-3 px-3 py-2">
        <h1 className="text-xs font-bold uppercase tracking-widest text-zinc-500">
          Racc
        </h1>
      </div>

      <ImportRepoDialog />

      <div className="flex-1 overflow-y-auto px-1 py-1">
        {repos.length === 0 && (
          <p className="px-3 py-4 text-center text-xs text-zinc-600">
            No repos imported yet
          </p>
        )}

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
                onClick={() => setAgentDialogRepoId(repo.id)}
                className="ml-1 hidden rounded px-1 text-xs text-zinc-500 transition-colors duration-150 hover:text-accent group-hover:block"
                title="Launch agent"
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
                    {session.status === "Running" ? (
                      <button
                        onClick={(e) => {
                          e.stopPropagation();
                          stopSession(session.id);
                        }}
                        className="hidden text-xs text-zinc-500 transition-colors duration-150 hover:text-red-400 group-hover:block"
                        title="Stop session"
                      >
                        ■
                      </button>
                    ) : (
                      <>
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
                      </>
                    )}
                  </div>
                  {sessionLastOutput[session.id] && (
                    <p className="mt-0.5 truncate text-[10px] text-zinc-600 leading-tight pl-3.5">
                      {sessionLastOutput[session.id]}
                    </p>
                  )}
                  {session.pr_url && (() => {
                    const pr = parsePrDisplay(session.pr_url);
                    return pr ? (
                      <button
                        onClick={(e) => {
                          e.stopPropagation();
                          open(session.pr_url!);
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

      <div className="border-t border-surface-3 px-3 py-2">
        <button
          onClick={() => setResetDialogOpen(true)}
          className="w-full rounded px-2 py-1.5 text-xs text-zinc-500 transition-colors duration-150 hover:bg-surface-2 hover:text-red-400"
        >
          Reset Database
        </button>
      </div>

      {agentDialogRepoId !== null && (
        <NewAgentDialog
          repoId={agentDialogRepoId}
          open={true}
          onClose={() => setAgentDialogRepoId(null)}
        />
      )}

      {removeDialogSession !== null && (
        <RemoveSessionDialog
          session={removeDialogSession}
          open={true}
          onClose={() => setRemoveDialogSession(null)}
        />
      )}

      <ResetDbDialog
        open={resetDialogOpen}
        onClose={() => setResetDialogOpen(false)}
      />
    </aside>
  );
}
