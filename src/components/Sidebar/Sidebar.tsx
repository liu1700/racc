import { useState } from "react";
import { useSessionStore } from "../../stores/sessionStore";
import { ImportRepoDialog } from "./ImportRepoDialog";
import { NewAgentDialog } from "./NewAgentDialog";
import type { SessionStatus } from "../../types/session";

const statusColor: Record<SessionStatus, string> = {
  Running: "bg-status-running",
  Completed: "bg-status-completed",
  Disconnected: "bg-status-disconnected",
  Error: "bg-status-error",
};

export function Sidebar() {
  const repos = useSessionStore((s) => s.repos);
  const activeSessionId = useSessionStore((s) => s.activeSessionId);
  const setActiveSession = useSessionStore((s) => s.setActiveSession);
  const stopSession = useSessionStore((s) => s.stopSession);
  const removeSession = useSessionStore((s) => s.removeSession);
  const removeRepo = useSessionStore((s) => s.removeRepo);

  const [expandedRepos, setExpandedRepos] = useState<Set<number>>(new Set());
  const [agentDialogRepoId, setAgentDialogRepoId] = useState<number | null>(null);

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
            <div className="group flex items-center rounded px-2 py-1.5 hover:bg-surface-2">
              <button
                onClick={() => toggleRepo(repo.id)}
                className="mr-1 text-xs text-zinc-500"
              >
                {isExpanded(repo.id) ? "▼" : "▶"}
              </button>
              <span
                className="flex-1 truncate text-xs font-medium text-zinc-300 cursor-pointer"
                onClick={() => toggleRepo(repo.id)}
                title={repo.path}
              >
                {repo.name}
              </span>
              <button
                onClick={() => setAgentDialogRepoId(repo.id)}
                className="ml-1 hidden rounded px-1 text-xs text-zinc-500 hover:text-accent group-hover:block"
                title="Launch agent"
              >
                +
              </button>
              <button
                onClick={() => removeRepo(repo.id)}
                className="ml-1 hidden rounded px-1 text-xs text-zinc-500 hover:text-red-400 group-hover:block"
                title="Remove repo"
              >
                ×
              </button>
            </div>

            {isExpanded(repo.id) &&
              sessions.map((session) => (
                <div
                  key={session.id}
                  onClick={() => {
                    if (session.status === "Running") {
                      setActiveSession(session.id);
                    }
                  }}
                  className={`group ml-4 flex cursor-pointer items-center gap-2 rounded px-2 py-1 ${
                    session.id === activeSessionId
                      ? "bg-surface-3"
                      : "hover:bg-surface-2"
                  }`}
                >
                  <span
                    className={`h-1.5 w-1.5 rounded-full ${statusColor[session.status]}`}
                  />
                  <span className="flex-1 truncate text-xs text-zinc-400">
                    {session.branch ?? "main"}
                  </span>
                  {session.status === "Running" ? (
                    <button
                      onClick={(e) => {
                        e.stopPropagation();
                        stopSession(session.id);
                      }}
                      className="hidden text-xs text-zinc-500 hover:text-red-400 group-hover:block"
                      title="Stop session"
                    >
                      ■
                    </button>
                  ) : (
                    <button
                      onClick={(e) => {
                        e.stopPropagation();
                        removeSession(session.id);
                      }}
                      className="hidden text-xs text-zinc-500 hover:text-red-400 group-hover:block"
                      title="Remove session"
                    >
                      ×
                    </button>
                  )}
                </div>
              ))}
          </div>
        ))}
      </div>

      {agentDialogRepoId !== null && (
        <NewAgentDialog
          repoId={agentDialogRepoId}
          open={true}
          onClose={() => setAgentDialogRepoId(null)}
        />
      )}
    </aside>
  );
}
