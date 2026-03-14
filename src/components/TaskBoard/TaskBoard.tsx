import { useEffect } from "react";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import type { TaskStatus } from "../../types/task";
import { useTaskStore } from "../../stores/taskStore";
import { useSessionStore } from "../../stores/sessionStore";
import { TaskColumn } from "./TaskColumn";

const COLUMNS: TaskStatus[] = ["open", "working", "closed"];

interface Props {
  repoId: number | null;
}

export function TaskBoard({ repoId }: Props) {
  const { tasks, createTask, loading, error, draftInputOpen, draftValue, setDraftInputOpen, setDraftValue } = useTaskStore();
  const repos = useSessionStore((s) => s.repos);

  // Note: loadTasks is called in App.tsx to support the tab badge.
  // No duplicate load here.

  // Watch session status changes → sync working→closed
  // Also detect orphaned working tasks whose session no longer exists
  useEffect(() => {
    const { syncTaskWithSession, updateTaskStatus, tasks: currentTasks } = useTaskStore.getState();
    const runningTasks = currentTasks.filter((t) => t.status === "working" && t.session_id);
    if (runningTasks.length === 0) return;

    // Build set of all existing session IDs
    const allSessionIds = new Set<number>();
    for (const repo of repos) {
      for (const session of repo.sessions) {
        allSessionIds.add(session.id);
      }
    }

    for (const task of runningTasks) {
      if (!allSessionIds.has(task.session_id!)) {
        // Session was removed — mark task as closed
        updateTaskStatus(task.id, "closed").catch(() => {});
      } else {
        // Session still exists — check if it completed
        for (const repo of repos) {
          const session = repo.sessions.find((s) => s.id === task.session_id);
          if (session) {
            syncTaskWithSession(session.id, session.status);
            break;
          }
        }
      }
    }
  }, [repos]);

  const importRepo = useSessionStore((s) => s.importRepo);

  if (!repoId) {
    return (
      <div className="flex flex-1 items-center justify-center">
        <button
          onClick={async () => {
            const selected = await openDialog({ directory: true, multiple: false });
            if (selected) await importRepo(selected);
          }}
          className="text-sm text-zinc-500 transition-colors hover:text-accent"
        >
          Select a git repo
        </button>
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
    <div className="grid flex-1 grid-cols-3 gap-2 overflow-x-auto p-3">
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
          inputOpen={status === "open" ? draftInputOpen : false}
          onInputOpenChange={status === "open" ? setDraftInputOpen : undefined}
          draftValue={status === "open" ? draftValue : ""}
          onDraftChange={status === "open" ? setDraftValue : undefined}
        />
      ))}
    </div>
  );
}
