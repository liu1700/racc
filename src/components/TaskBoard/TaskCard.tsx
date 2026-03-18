import { useState, useRef, useMemo, useEffect } from "react";
import { transport } from "../../services/transport";
import type { Task } from "../../types/task";

function getAssetUrl(path: string): string {
  if (transport.isLocal()) {
    return `asset://localhost/${encodeURIComponent(path)}`;
  }
  return path;
}

function openUrl(url: string): void {
  if (transport.isLocal()) {
    import("@tauri-apps/plugin-shell").then((m) => m.open(url));
  } else {
    window.open(url, "_blank");
  }
}
import { useSessionStore } from "../../stores/sessionStore";
import { useTaskStore } from "../../stores/taskStore";
import { FireTaskDialog } from "./FireTaskDialog";
import { parsePrDisplay } from "../../utils/prUrl";
import { useIMEComposition } from "../../hooks/useIMEComposition";

interface Props {
  task: Task;
  onSessionSelect?: () => void;
}

function formatElapsed(createdAt: string): string {
  const diff = Date.now() - new Date(createdAt + "Z").getTime();
  const mins = Math.floor(diff / 60000);
  if (mins < 1) return "<1m";
  if (mins < 60) return `${mins}m`;
  return `${Math.floor(mins / 60)}h ${mins % 60}m`;
}

export function TaskCard({ task, onSessionSelect }: Props) {
  const [fireOpen, setFireOpen] = useState(false);
  const [editing, setEditing] = useState(false);
  const [editValue, setEditValue] = useState(task.description);
  const editRef = useRef<HTMLTextAreaElement>(null);
  const { isComposingRef, compositionProps } = useIMEComposition();
  const sessionLastOutput = useSessionStore((s) => s.sessionLastOutput);
  const repos = useSessionStore((s) => s.repos);
  const setActiveSession = useSessionStore((s) => s.setActiveSession);
  const updateTaskDescription = useTaskStore((s) => s.updateTaskDescription);
  const deleteTask = useTaskStore((s) => s.deleteTask);

  useEffect(() => {
    if (editing) {
      editRef.current?.focus();
    }
  }, [editing]);

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

  const repoPath = useMemo(() => {
    const repo = repos.find((r) => r.repo.id === task.repo_id);
    return repo?.repo.path ?? "";
  }, [repos, task.repo_id]);

  const prDisplay = useMemo(() => {
    if (!linkedSession?.pr_url) return null;
    return parsePrDisplay(linkedSession.pr_url);
  }, [linkedSession?.pr_url]);

  const statusBorder = {
    open: "border-l-accent",
    working: "border-l-status-running",
    closed: "border-l-status-completed",
  }[task.status];

  const handleEditSave = () => {
    const trimmed = editValue.trim();
    if (trimmed && trimmed !== task.description) {
      updateTaskDescription(task.id, trimmed);
    }
    setEditing(false);
  };

  const handleEditKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === "Enter" && !e.shiftKey && !isComposingRef.current) {
      e.preventDefault();
      handleEditSave();
    }
    if (e.key === "Escape") {
      setEditValue(task.description);
      setEditing(false);
    }
  };

  const handleDescriptionClick = () => {
    if (task.status === "open") {
      setEditValue(task.description);
      setEditing(true);
    }
  };

  return (
    <>
      <div
        className={`group/card relative min-w-0 overflow-hidden rounded border border-surface-3 border-l-2 ${statusBorder} bg-surface-1 p-2.5 transition-colors hover:bg-surface-2 ${
          task.status === "closed" ? "opacity-50" : ""
        } ${task.status === "working" ? "cursor-pointer" : ""}`}
        onClick={
          task.status === "working" && task.session_id
            ? () => {
                setActiveSession(task.session_id!);
                onSessionSelect?.();
              }
            : undefined
        }
      >
        {task.status === "open" && (
          <button
            onClick={(e) => {
              e.stopPropagation();
              deleteTask(task.id);
            }}
            className="absolute right-1.5 top-1.5 hidden rounded px-1 text-xs text-zinc-500 transition-colors hover:text-red-400 group-hover/card:block"
            title="Delete task"
          >
            ×
          </button>
        )}
        {editing ? (
          <textarea
            ref={editRef}
            value={editValue}
            onChange={(e) => setEditValue(e.target.value)}
            onKeyDown={handleEditKeyDown}
            {...compositionProps}
            onBlur={handleEditSave}
            rows={3}
            className="mb-1 w-full resize-none rounded border border-accent bg-surface-2 px-1.5 py-1 text-xs font-medium leading-snug text-zinc-200 outline-none"
          />
        ) : (
          <p
            className={`mb-1 whitespace-pre-wrap text-xs font-medium leading-snug text-zinc-200 ${
              task.status === "open" ? "cursor-text hover:text-white" : ""
            }`}
            onClick={handleDescriptionClick}
          >
            {task.description}
          </p>
        )}

        {task.images.length > 0 && repoPath && (
          <div className="mb-1 flex flex-wrap gap-1">
            {task.images.map((img) => (
              <img
                key={img}
                src={getAssetUrl(`${repoPath}/.racc/images/${img}`)}
                alt=""
                className="h-8 w-8 rounded border border-surface-3 object-cover"
              />
            ))}
          </div>
        )}

        {/* Working: show linked session + live activity + elapsed time */}
        {task.status === "working" && (
          <>
            <div className="mb-1 flex min-w-0 items-center gap-1.5 text-[10px] text-status-running">
              <span className="inline-block h-1 w-1 flex-shrink-0 animate-status-pulse rounded-full bg-status-running" />
              <span className="truncate">
                {linkedSession?.branch ?? "session"}
              </span>
              {lastOutput && (
                <span className="min-w-0 flex-1 truncate text-status-running/60">
                  — {lastOutput}
                </span>
              )}
              {prDisplay && (
                <button
                  onClick={(e) => {
                    e.stopPropagation();
                    openUrl(linkedSession!.pr_url!);
                  }}
                  className="ml-1 shrink-0 text-accent hover:underline"
                >
                  {prDisplay.label}
                </button>
              )}
            </div>
            <div className="flex items-center gap-2 text-[10px] text-zinc-500">
              <span>{formatElapsed(task.updated_at)}</span>
            </div>
          </>
        )}

        {/* Open: show fire button */}
        {task.status === "open" && (
          <div className="flex items-center gap-2 text-[10px] text-zinc-500">
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
            {prDisplay && (
              <button
                onClick={(e) => {
                  e.stopPropagation();
                  open(linkedSession!.pr_url!);
                }}
                className="ml-1 text-accent hover:underline"
              >
                {prDisplay.label}
              </button>
            )}
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
