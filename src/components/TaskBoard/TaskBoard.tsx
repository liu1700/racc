import { useEffect, useMemo, useState } from "react";
import { transport } from "../../services/transport";
import type { TaskStatus } from "../../types/task";
import { useTaskStore } from "../../stores/taskStore";
import { useMergeStore } from "../../stores/mergeStore";
import { useSessionStore } from "../../stores/sessionStore";
import { usePlannerStore } from "../../stores/plannerStore";
import { TaskColumn } from "./TaskColumn";
import { MergeManagerColumn } from "./MergeManagerColumn";
import { TaskPlannerDialog } from "./TaskPlannerDialog";

const COLUMNS: TaskStatus[] = ["open", "working", "closed"];

interface Props {
  repoId: number | null;
  onSessionSelect?: () => void;
}

export function TaskBoard({ repoId, onSessionSelect }: Props) {
  const [plannerOpen, setPlannerOpen] = useState(false);
  const {
    tasks,
    createTask,
    loading,
    error,
    draftInputOpen,
    draftValue,
    draftImages,
    setDraftInputOpen,
    setDraftValue,
    addDraftImage,
    removeDraftImage,
  } = useTaskStore();
  const repos = useSessionStore((s) => s.repos);
  const initializeMergeEvents = useMergeStore((s) => s.initializeEvents);
  const loadMergeManager = useMergeStore((s) => s.load);
  const initializePlannerEvents = usePlannerStore((s) => s.initializeEvents);

  useEffect(() => {
    initializeMergeEvents();
    initializePlannerEvents();
  }, [initializeMergeEvents, initializePlannerEvents]);

  useEffect(() => {
    if (repoId) void loadMergeManager(repoId);
  }, [repoId, loadMergeManager]);

  const repoPath = useMemo(() => {
    if (!repoId) return "";
    const r = repos.find((rr) => rr.repo.id === repoId);
    return r?.repo.path ?? "";
  }, [repoId, repos]);

  // Note: loadTasks is called in App.tsx to support the tab badge.
  // No duplicate load here.

  // Watch session status changes → sync working→closed
  // Also detect orphaned working tasks whose session no longer exists
  useEffect(() => {
    const {
      syncTaskWithSession,
      updateTaskStatus,
      tasks: currentTasks,
    } = useTaskStore.getState();
    const runningTasks = currentTasks.filter(
      (t) => t.status === "working" && t.session_id
    );
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
            if (transport.isLocal()) {
              const { open } = await import("@tauri-apps/plugin-dialog");
              const selected = await open({
                directory: true,
                multiple: false,
              });
              if (selected) await importRepo(selected);
            }
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
    COLUMNS.map((status) => [
      status,
      tasks.filter((t) => t.status === status),
    ])
  ) as Record<TaskStatus, typeof tasks>;

  const renderTaskColumn = (status: TaskStatus) => (
    <TaskColumn
      key={status}
      status={status}
      tasks={tasksByStatus[status]}
      repoPath={repoPath}
      onSessionSelect={onSessionSelect}
      onCreateTask={status === "open" ? (desc) => createTask(repoId, desc) : undefined}
      onGenerateTasks={status === "open" ? () => setPlannerOpen(true) : undefined}
      inputOpen={status === "open" ? draftInputOpen : false}
      onInputOpenChange={status === "open" ? setDraftInputOpen : undefined}
      draftValue={status === "open" ? draftValue : ""}
      onDraftChange={status === "open" ? setDraftValue : undefined}
      draftImages={status === "open" ? draftImages : []}
      onAddImage={status === "open" ? addDraftImage : undefined}
      onRemoveImage={status === "open" ? removeDraftImage : undefined}
    />
  );

  return (
    <>
      <div className="flex-1 overflow-x-auto">
        <div className="grid h-full min-w-[1080px] grid-cols-[minmax(220px,1fr)_minmax(240px,1fr)_minmax(300px,1.15fr)_minmax(220px,1fr)] gap-2 p-3">
          {renderTaskColumn("open")}
          {renderTaskColumn("working")}
          <MergeManagerColumn repoId={repoId} onSessionSelect={onSessionSelect} />
          {renderTaskColumn("closed")}
        </div>
      </div>
      <TaskPlannerDialog
        repoId={repoId}
        open={plannerOpen}
        onClose={() => setPlannerOpen(false)}
        onSessionSelect={onSessionSelect}
      />
    </>
  );
}
