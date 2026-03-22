import { useMemo } from "react";
import { useTaskStore } from "../stores/taskStore";
import { useSessionStore } from "../stores/sessionStore";
import { transport } from "../services/transport";
import type { Task } from "../types/task";

interface SupervisorDashboardProps {
  repoId: number | null;
}

type SupervisorStatus = "pending" | "running" | "completed" | "failed" | "needs_input";

const STATUS_CONFIG: Record<
  SupervisorStatus,
  { label: string; colorClass: string; dotClass: string }
> = {
  pending: {
    label: "Pending",
    colorClass: "text-zinc-400",
    dotClass: "bg-zinc-400",
  },
  running: {
    label: "Running",
    colorClass: "text-status-running",
    dotClass: "bg-status-running",
  },
  completed: {
    label: "Completed",
    colorClass: "text-status-completed",
    dotClass: "bg-status-completed",
  },
  failed: {
    label: "Failed",
    colorClass: "text-status-error",
    dotClass: "bg-status-error",
  },
  needs_input: {
    label: "NeedsInput",
    colorClass: "text-yellow-400",
    dotClass: "bg-yellow-400",
  },
};

function getSupervisorStatus(task: Task): SupervisorStatus {
  if (task.supervisor_status) {
    const s = task.supervisor_status as SupervisorStatus;
    if (s in STATUS_CONFIG) return s;
  }
  // Derive from task status if no supervisor_status set
  if (task.status === "closed") return "completed";
  if (task.status === "working") return "running";
  return "pending";
}

export function SupervisorDashboard({ repoId }: SupervisorDashboardProps) {
  const tasks = useTaskStore((s) => s.tasks);
  const repos = useSessionStore((s) => s.repos);

  const supervisedTasks = useMemo(
    () => tasks.filter((t) => !repoId || t.repo_id === repoId),
    [tasks, repoId]
  );

  const counts = useMemo(() => {
    const c = { pending: 0, running: 0, completed: 0, failed: 0, needs_input: 0 };
    for (const task of supervisedTasks) {
      const status = getSupervisorStatus(task);
      c[status]++;
    }
    return c;
  }, [supervisedTasks]);

  const handleRetry = async (task: Task) => {
    try {
      await transport.call("update_task_status", {
        taskId: task.id,
        status: "open",
        sessionId: null,
      });
      // Reload tasks to reflect the change
      if (repoId) {
        useTaskStore.getState().loadTasks(repoId);
      }
    } catch (err) {
      console.error("Failed to retry task:", err);
    }
  };

  const handleTerminal = (sessionId: number) => {
    useSessionStore.getState().setActiveSession(sessionId);
  };

  const getSessionForTask = (task: Task) => {
    if (!task.session_id) return null;
    for (const r of repos) {
      const session = r.sessions.find((s) => s.id === task.session_id);
      if (session) return session;
    }
    return null;
  };

  if (supervisedTasks.length === 0) {
    return (
      <div className="flex flex-1 items-center justify-center p-8 text-zinc-500">
        <p>No supervised tasks. Create a task and the supervisor will auto-assign it.</p>
      </div>
    );
  }

  return (
    <div className="flex flex-1 flex-col overflow-y-auto p-4">
      {/* Summary counters */}
      <div className="mb-4 flex gap-3">
        {(Object.keys(STATUS_CONFIG) as SupervisorStatus[]).map((key) => {
          const config = STATUS_CONFIG[key];
          return (
            <div
              key={key}
              className="flex items-center gap-2 rounded-md bg-surface-1 px-3 py-2"
            >
              <span className={`h-2 w-2 rounded-full ${config.dotClass}`} />
              <span className="text-xs text-zinc-400">{config.label}</span>
              <span className={`text-sm font-semibold ${config.colorClass}`}>
                {counts[key]}
              </span>
            </div>
          );
        })}
      </div>

      {/* Task list */}
      <div className="flex flex-col gap-2">
        {supervisedTasks.map((task) => {
          const status = getSupervisorStatus(task);
          const config = STATUS_CONFIG[status];
          const session = getSessionForTask(task);

          return (
            <div
              key={task.id}
              className="flex items-center gap-3 rounded-lg bg-surface-1 px-4 py-3"
            >
              {/* Status indicator */}
              <span
                className={`h-2.5 w-2.5 shrink-0 rounded-full ${config.dotClass}`}
                title={config.label}
              />

              {/* Task info */}
              <div className="min-w-0 flex-1">
                <p className="truncate text-sm text-zinc-200">
                  {task.description}
                </p>
                <div className="mt-1 flex items-center gap-3 text-xs text-zinc-500">
                  <span className={config.colorClass}>{config.label}</span>
                  {session && (
                    <>
                      <span>
                        {session.agent}
                        {session.branch ? ` / ${session.branch}` : ""}
                      </span>
                    </>
                  )}
                  {task.retry_count > 0 && (
                    <span className="text-yellow-400">
                      Retries: {task.retry_count}/{task.max_retries}
                    </span>
                  )}
                </div>
              </div>

              {/* Action buttons */}
              <div className="flex shrink-0 gap-2">
                {status === "failed" && (
                  <button
                    onClick={() => handleRetry(task)}
                    className="rounded bg-accent/20 px-2.5 py-1 text-xs text-accent hover:bg-accent/30 transition-colors"
                  >
                    Retry
                  </button>
                )}
                {session && (
                  <button
                    onClick={() => handleTerminal(session.id)}
                    className="rounded bg-surface-2 px-2.5 py-1 text-xs text-zinc-300 hover:bg-surface-3 transition-colors"
                  >
                    Terminal
                  </button>
                )}
              </div>
            </div>
          );
        })}
      </div>
    </div>
  );
}
