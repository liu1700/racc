import { useState, useMemo } from "react";
import type { Task } from "../../types/task";
import { useSessionStore } from "../../stores/sessionStore";
import { useTaskStore } from "../../stores/taskStore";
import { FireTaskDialog } from "./FireTaskDialog";

interface Props {
  task: Task;
  onSwitchToTerminal: () => void;
}

function formatElapsed(createdAt: string): string {
  const diff = Date.now() - new Date(createdAt + "Z").getTime();
  const mins = Math.floor(diff / 60000);
  if (mins < 1) return "<1m";
  if (mins < 60) return `${mins}m`;
  return `${Math.floor(mins / 60)}h ${mins % 60}m`;
}

export function TaskCard({ task, onSwitchToTerminal }: Props) {
  const [fireOpen, setFireOpen] = useState(false);
  const sessionActivities = useSessionStore((s) => s.sessionActivities);
  const setActiveSession = useSessionStore((s) => s.setActiveSession);
  const repos = useSessionStore((s) => s.repos);
  const updateTaskStatus = useTaskStore((s) => s.updateTaskStatus);

  const activity = task.session_id
    ? sessionActivities[task.session_id]
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
    running: "border-l-status-running",
    review: "border-l-status-waiting",
    done: "border-l-status-completed",
  }[task.status];

  const handleReviewClick = () => {
    if (task.session_id) {
      setActiveSession(task.session_id);
      onSwitchToTerminal();
    }
  };

  const handleMarkDone = (e: React.MouseEvent) => {
    e.stopPropagation();
    updateTaskStatus(task.id, "done");
  };

  return (
    <>
      <div
        className={`rounded border border-surface-3 border-l-2 ${statusBorder} bg-surface-1 p-2.5 transition-colors hover:bg-surface-2 ${
          task.status === "done" ? "opacity-50" : ""
        } ${task.status === "review" ? "cursor-pointer" : ""}`}
        onClick={task.status === "review" ? handleReviewClick : undefined}
      >
        <p className="mb-1 text-xs font-medium leading-snug text-zinc-200">
          {task.description}
        </p>

        {/* Running: show linked session + live activity + elapsed time */}
        {task.status === "running" && (
          <>
            {activity && (
              <div className="mb-1 flex items-center gap-1.5 text-[10px] text-status-running">
                <span className="inline-block h-1 w-1 animate-status-pulse rounded-full bg-status-running" />
                <span className="truncate">
                  {linkedSession?.branch ?? "session"}
                  {" — "}
                  {activity.action}
                  {activity.detail ? ` ${activity.detail}` : ""}
                </span>
              </div>
            )}
            <div className="flex items-center gap-2 text-[10px] text-zinc-500">
              <span className="rounded bg-surface-2 px-1.5 py-0.5">claude</span>
              <span>{formatElapsed(task.updated_at)}</span>
            </div>
          </>
        )}

        {/* Review: show diff summary hint + done button */}
        {task.status === "review" && (
          <>
            <div className="mb-1 text-[10px] italic text-zinc-500">
              Click to review in terminal
            </div>
            <div className="flex items-center justify-between">
              <span className="text-[10px] text-zinc-500">
                {linkedSession?.branch ?? "session"} · done {formatElapsed(task.updated_at)} ago
              </span>
              <button
                onClick={handleMarkDone}
                className="rounded bg-status-completed/15 px-1.5 py-0.5 text-[10px] text-status-completed hover:bg-status-completed/25"
              >
                Done
              </button>
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

        {/* Done: minimal meta */}
        {task.status === "done" && (
          <div className="text-[10px] text-zinc-600">
            {linkedSession?.branch ?? "session"} · completed
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
