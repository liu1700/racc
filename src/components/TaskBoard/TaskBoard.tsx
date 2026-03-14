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
  const { tasks, loadTasks, createTask } = useTaskStore();
  const repos = useSessionStore((s) => s.repos);

  // Load tasks when repo changes
  useEffect(() => {
    if (repoId) loadTasks(repoId);
  }, [repoId, loadTasks]);

  // Watch session status changes → sync running→review
  useEffect(() => {
    const syncTaskWithSession = useTaskStore.getState().syncTaskWithSession;
    for (const repo of repos) {
      for (const session of repo.sessions) {
        syncTaskWithSession(session.id, session.status);
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

  const tasksByStatus = Object.fromEntries(
    COLUMNS.map((status) => [status, tasks.filter((t) => t.status === status)])
  ) as Record<TaskStatus, typeof tasks>;

  return (
    <div className="flex flex-1 gap-2 overflow-x-auto p-3">
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
