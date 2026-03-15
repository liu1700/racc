import { invoke } from "@tauri-apps/api/core";
import type { Task, TaskStatus, DraftImage } from "../../types/task";
import { useTaskStore } from "../../stores/taskStore";
import { TaskCard } from "./TaskCard";
import { TaskInput } from "./TaskInput";

const COLUMN_CONFIG: Record<
  TaskStatus,
  { label: string; dotColor: string }
> = {
  open: { label: "Open", dotColor: "bg-accent" },
  working: { label: "Working", dotColor: "bg-status-running" },
  closed: { label: "Closed", dotColor: "bg-status-completed" },
};

interface Props {
  status: TaskStatus;
  tasks: Task[];
  repoPath: string;
  onCreateTask?: (description: string) => Promise<Task>;
  inputOpen?: boolean;
  onInputOpenChange?: (open: boolean) => void;
  draftValue?: string;
  onDraftChange?: (value: string) => void;
  draftImages?: DraftImage[];
  onAddImage?: (image: DraftImage) => void;
  onRemoveImage?: (filename: string) => void;
}

export function TaskColumn({
  status,
  tasks,
  repoPath,
  onCreateTask,
  inputOpen = false,
  onInputOpenChange,
  draftValue = "",
  onDraftChange,
  draftImages = [],
  onAddImage,
  onRemoveImage,
}: Props) {
  const config = COLUMN_CONFIG[status];
  const clearDraftImages = useTaskStore((s) => s.clearDraftImages);

  const handleSubmit = async (desc: string) => {
    if (!onCreateTask) return;
    const task = await onCreateTask(desc);

    // Rename draft images to task-id-based names
    const renamedImages: string[] = [];
    for (let i = 0; i < draftImages.length; i++) {
      const draft = draftImages[i];
      const ext = draft.filename.split(".").pop() || "png";
      const newName = `${task.id}-${Date.now()}-${i}.${ext}`;
      await invoke("rename_task_image", {
        repoPath,
        oldName: draft.filename,
        newName,
      });
      renamedImages.push(newName);
    }
    if (renamedImages.length > 0) {
      await invoke("update_task_images", {
        taskId: task.id,
        images: JSON.stringify(renamedImages),
      });
      // Reload tasks to get updated images
      const { loadTasks } = useTaskStore.getState();
      await loadTasks(task.repo_id);
    }
    clearDraftImages();
    onDraftChange?.("");
    onInputOpenChange?.(false);
  };

  return (
    <div className="flex min-w-0 flex-col gap-1.5 overflow-hidden">
      {/* Column header */}
      <div className="mb-1 flex items-center gap-2 px-2 py-1">
        <span className={`h-1.5 w-1.5 rounded-full ${config.dotColor}`} />
        <span className="text-[10px] uppercase tracking-wider text-zinc-500">
          {config.label}
        </span>
        <span className="text-[10px] text-zinc-600">{tasks.length}</span>
      </div>

      {/* Cards */}
      <div className="flex flex-col gap-1.5 overflow-y-auto px-1">
        {tasks.map((task) => (
          <TaskCard
            key={task.id}
            task={task}
          />
        ))}
      </div>

      {/* New task input (Open column only) */}
      {onCreateTask && (
        <div className="px-1">
          {inputOpen ? (
            <TaskInput
              value={draftValue}
              onChange={(v) => onDraftChange?.(v)}
              onSubmit={handleSubmit}
              onCancel={() => {
                onInputOpenChange?.(false);
              }}
              repoPath={repoPath}
              images={draftImages}
              onAddImage={(img) => onAddImage?.(img)}
              onRemoveImage={(fn) => onRemoveImage?.(fn)}
            />
          ) : (
            <button
              onClick={() => onInputOpenChange?.(true)}
              className="w-full rounded border border-dashed border-surface-3 py-1.5 text-center text-[10px] text-zinc-600 transition-colors hover:border-accent hover:text-accent"
            >
              + New Task
            </button>
          )}
        </div>
      )}
    </div>
  );
}
