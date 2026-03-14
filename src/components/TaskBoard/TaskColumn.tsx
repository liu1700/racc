import type { Task, TaskStatus } from "../../types/task";
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
  onCreateTask?: (description: string) => void;
  inputOpen?: boolean;
  onInputOpenChange?: (open: boolean) => void;
  draftValue?: string;
  onDraftChange?: (value: string) => void;
}

export function TaskColumn({
  status,
  tasks,
  onCreateTask,
  inputOpen = false,
  onInputOpenChange,
  draftValue = "",
  onDraftChange,
}: Props) {
  const config = COLUMN_CONFIG[status];

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
              onSubmit={(desc) => {
                onCreateTask(desc);
                onDraftChange?.("");
                onInputOpenChange?.(false);
              }}
              onCancel={() => {
                onInputOpenChange?.(false);
              }}
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
