import { useEffect } from "react";
import type { TaskStatus } from "../../types/task";
import { useTaskStore } from "../../stores/taskStore";
import { useSessionStore } from "../../stores/sessionStore";
import { TaskColumn } from "./TaskColumn";

const COLUMNS: TaskStatus[] = ["open", "running", "review", "done"];

interface Props {
  repoId: number | null;
  onSwitchToTerminal: () => void;
}

export function TaskBoard({ repoId, onSwitchToTerminal }: Props) {
  const { tasks, createTask, loading, error } = useTaskStore();
  const repos = useSessionStore((s) => s.repos);

  // Note: loadTasks is called in App.tsx to support the tab badge.
  // No duplicate load here.

  // Watch session status changes → sync running→review
  // Only check sessions linked to running tasks to avoid O(N*M) scan
  useEffect(() => {
    const { syncTaskWithSession, tasks: currentTasks } = useTaskStore.getState();
    const runningSessionIds = new Set(
      currentTasks
        .filter((t) => t.status === "running" && t.session_id)
        .map((t) => t.session_id!)
    );
    if (runningSessionIds.size === 0) return;

    for (const repo of repos) {
      for (const session of repo.sessions) {
        if (runningSessionIds.has(session.id)) {
          syncTaskWithSession(session.id, session.status);
        }
      }
    }
  }, [repos]);

  if (!repoId) {
    return (
      <div className="flex flex-1 items-center justify-center text-sm text-zinc-600">
        Select a repo to view tasks
      </div>
    );
  }

  if (loading && tasks.length === 0) {
    return (
      <div className="flex flex-1 items-center justify-center text-sm text-zinc-500">
        Loading tasks...
      </div>
    );
  }

  if (error) {
    return (
      <div className="flex flex-1 items-center justify-center text-sm text-red-400">
        {error}
      </div>
    );
  }

  const tasksByStatus = Object.fromEntries(
    COLUMNS.map((status) => [status, tasks.filter((t) => t.status === status)])
  ) as Record<TaskStatus, typeof tasks>;

  return (
    <div className="grid flex-1 grid-cols-4 gap-2 overflow-x-auto p-3">
      {COLUMNS.map((status) => (
        <TaskColumn
          key={status}
          status={status}
          tasks={tasksByStatus[status]}
          onCreateTask={
            status === "open"
              ? (desc) => createTask(repoId, desc)
              : undefined
          }
          onSwitchToTerminal={onSwitchToTerminal}
        />
      ))}
    </div>
  );
}
