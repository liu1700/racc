import { useState, useMemo } from "react";
import type { Task } from "../../types/task";
import { useSessionStore } from "../../stores/sessionStore";
import { FireTaskDialog } from "./FireTaskDialog";

interface Props {
  task: Task;
}

function formatElapsed(createdAt: string): string {
  const diff = Date.now() - new Date(createdAt + "Z").getTime();
  const mins = Math.floor(diff / 60000);
  if (mins < 1) return "<1m";
  if (mins < 60) return `${mins}m`;
  return `${Math.floor(mins / 60)}h ${mins % 60}m`;
}

export function TaskCard({ task }: Props) {
  const [fireOpen, setFireOpen] = useState(false);
  const sessionLastOutput = useSessionStore((s) => s.sessionLastOutput);
  const repos = useSessionStore((s) => s.repos);

  const lastOutput = task.session_id
    ? sessionLastOutput[task.session_id] ?? null
    : null;

  // Find linked session for branch name display
  const linkedSession = useMemo(() => {
    if (!task.session_id) return null;
    for (const repo of repos) {
      const session = repo.sessions.find((s) => s.id === task.session_id);
      if (session) return session;
    }
    return null;
  }, [task.session_id, repos]);

  const statusBorder = {
    open: "border-l-accent",
    working: "border-l-status-running",
    closed: "border-l-status-completed",
  }[task.status];

  return (
    <>
      <div
        className={`rounded border border-surface-3 border-l-2 ${statusBorder} bg-surface-1 p-2.5 transition-colors hover:bg-surface-2 ${
          task.status === "closed" ? "opacity-50" : ""
        }`}
      >
        <p className="mb-1 text-xs font-medium leading-snug text-zinc-200">
          {task.description}
        </p>

        {/* Working: show linked session + live activity + elapsed time */}
        {task.status === "working" && (
          <>
            <div className="mb-1 flex items-center gap-1.5 text-[10px] text-status-running">
              <span className="inline-block h-1 w-1 animate-status-pulse rounded-full bg-status-running" />
              <span className="truncate">
                {linkedSession?.branch ?? "session"}
                {lastOutput ? ` — ${lastOutput}` : ""}
              </span>
            </div>
            <div className="flex items-center gap-2 text-[10px] text-zinc-500">
              <span className="rounded bg-surface-2 px-1.5 py-0.5">claude</span>
              <span>{formatElapsed(task.updated_at)}</span>
            </div>
          </>
        )}

        {/* Open: show fire button */}
        {task.status === "open" && (
          <div className="flex items-center gap-2 text-[10px] text-zinc-500">
            <span className="rounded bg-surface-2 px-1.5 py-0.5">claude</span>
            <button
              onClick={() => setFireOpen(true)}
              className="ml-auto rounded bg-accent/15 px-2 py-0.5 text-accent hover:bg-accent/25"
            >
              Fire
            </button>
          </div>
        )}

        {/* Closed: minimal meta */}
        {task.status === "closed" && (
          <div className="text-[10px] text-zinc-600">
            {linkedSession?.branch ?? "session"} · closed
          </div>
        )}
      </div>

      <FireTaskDialog
        task={task}
        open={fireOpen}
        onClose={() => setFireOpen(false)}
      />
    </>
  );
}
