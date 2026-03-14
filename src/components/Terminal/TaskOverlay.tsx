import { useState } from "react";
import { useSessionStore } from "../../stores/sessionStore";
import { useTaskStore } from "../../stores/taskStore";
import { useShallow } from "zustand/react/shallow";

export function TaskOverlay() {
  const [collapsed, setCollapsed] = useState(false);
  const activeSession = useSessionStore(useShallow((s) => s.getActiveSession()));
  const tasks = useTaskStore((s) => s.tasks);

  const sessionId = activeSession?.session.id ?? null;
  const task = sessionId
    ? tasks.find((t) => t.session_id === sessionId)
    : null;

  if (!task) return null;

  const statusLabel =
    task.status === "open" ? "Open" : task.status === "working" ? "Working" : "Closed";
  const statusColor =
    task.status === "working"
      ? "bg-status-running"
      : task.status === "closed"
        ? "bg-status-completed"
        : "bg-yellow-500";

  if (collapsed) {
    return (
      <button
        onClick={() => setCollapsed(false)}
        className="absolute top-2 right-2 z-20 flex items-center gap-1.5 rounded bg-surface-2/90 px-2 py-1 text-[11px] text-zinc-400 backdrop-blur-sm border border-surface-3 hover:bg-surface-3 transition-colors"
        title={task.description}
      >
        <span className={`h-1.5 w-1.5 rounded-full ${statusColor}`} />
        <span>Task #{task.id}</span>
        <span className="text-zinc-600">›</span>
      </button>
    );
  }

  return (
    <div className="absolute top-2 right-2 z-20 max-w-xs rounded border border-surface-3 bg-surface-2/90 backdrop-blur-sm shadow-lg">
      <div className="flex items-center gap-2 px-2.5 py-1.5 border-b border-surface-3">
        <span className={`h-1.5 w-1.5 flex-shrink-0 rounded-full ${statusColor}`} />
        <span className="flex-1 text-[11px] font-medium text-zinc-300">
          Task #{task.id}
        </span>
        <span className="text-[10px] text-zinc-500">{statusLabel}</span>
        <button
          onClick={() => setCollapsed(true)}
          className="ml-1 text-zinc-500 hover:text-zinc-300 transition-colors text-xs leading-none"
          title="Collapse"
        >
          ‹
        </button>
      </div>
      <div className="px-2.5 py-2 text-[11px] text-zinc-400 leading-relaxed whitespace-pre-wrap break-words max-h-32 overflow-y-auto">
        {task.description}
      </div>
    </div>
  );
}
