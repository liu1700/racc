import { useEffect, useMemo, useState } from "react";
import { usePlannerStore } from "../../stores/plannerStore";
import { useSessionStore } from "../../stores/sessionStore";
import type { TaskPlanResult } from "../../types/planner";

interface Props {
  repoId: number;
  open: boolean;
  onClose: () => void;
  onSessionSelect?: () => void;
}

function parseResult(resultJson: string | null): TaskPlanResult | null {
  if (!resultJson) return null;
  try {
    return JSON.parse(resultJson) as TaskPlanResult;
  } catch {
    return null;
  }
}

export function TaskPlannerDialog({
  repoId,
  open,
  onClose,
  onSessionSelect,
}: Props) {
  const {
    run,
    loading,
    starting,
    confirming,
    error,
    load,
    start,
    confirm,
    clearError,
  } = usePlannerStore();
  const openSession = useSessionStore((state) => state.openSession);
  const [sourceInput, setSourceInput] = useState("");
  const [agent, setAgent] = useState<"claude-code" | "codex">("claude-code");
  const [selectedKeys, setSelectedKeys] = useState<Set<string>>(new Set());
  const [composeNew, setComposeNew] = useState(false);

  const result = useMemo(
    () => parseResult(run?.result_json ?? null),
    [run?.result_json],
  );

  useEffect(() => {
    if (!open) return;
    clearError();
    setComposeNew(false);
    void load(repoId);
  }, [open, repoId, load, clearError]);

  useEffect(() => {
    if (!result) {
      setSelectedKeys(new Set());
      return;
    }
    // Preview is opt-in: a task is created only after the user checks it.
    setSelectedKeys(new Set());
  }, [run?.id, result]);

  if (!open) return null;

  const active = run?.repo_id === repoId &&
    (run.status === "starting" || run.status === "planning");
  const ready = !composeNew && run?.repo_id === repoId && run.status === "ready";
  const showComposer = composeNew || (!active && !ready);
  const selectedCount = selectedKeys.size;

  const handleGenerate = async (event: React.FormEvent) => {
    event.preventDefault();
    if (!sourceInput.trim()) return;
    try {
      await start(repoId, sourceInput.trim(), agent);
      setComposeNew(false);
    } catch {
      // The store exposes the backend error in the dialog.
    }
  };

  const handleConfirm = async () => {
    try {
      await confirm(Array.from(selectedKeys));
      setSourceInput("");
      onClose();
    } catch {
      // Keep the preview open so the user can adjust the selection.
    }
  };

  const handleOpenTerminal = async () => {
    if (!run?.session_id) return;
    await openSession(run.session_id);
    onClose();
    onSessionSelect?.();
  };

  const toggleTask = (key: string) => {
    setSelectedKeys((current) => {
      const next = new Set(current);
      if (next.has(key)) {
        next.delete(key);
        // Removing a prerequisite also removes tasks that depend on it.
        let changed = true;
        while (changed) {
          changed = false;
          for (const task of result?.tasks ?? []) {
            if (
              next.has(task.key) &&
              task.depends_on.some((dependency) => !next.has(dependency))
            ) {
              next.delete(task.key);
              changed = true;
            }
          }
        }
      } else {
        // Selecting a dependent task automatically includes its prerequisites.
        const addWithDependencies = (taskKey: string) => {
          if (next.has(taskKey)) return;
          next.add(taskKey);
          const task = result?.tasks.find((candidate) => candidate.key === taskKey);
          for (const dependency of task?.depends_on ?? []) {
            addWithDependencies(dependency);
          }
        };
        addWithDependencies(key);
      }
      return next;
    });
  };

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/65 p-6"
      onMouseDown={(event) => {
        if (event.target === event.currentTarget && !starting && !confirming) onClose();
      }}
    >
      <div className="flex max-h-[88vh] w-full max-w-3xl flex-col overflow-hidden rounded-lg border border-surface-3 bg-surface-1 shadow-2xl">
        <div className="flex items-center gap-3 border-b border-surface-3 px-5 py-4">
          <div>
            <h2 className="text-sm font-semibold text-zinc-200">Generate tasks with AI</h2>
            <p className="mt-0.5 text-[10px] text-zinc-500">
              Paste an Epic link or a product description, then review before creating.
            </p>
          </div>
          <button
            onClick={onClose}
            disabled={confirming}
            className="ml-auto rounded px-2 text-lg text-zinc-600 hover:text-zinc-300 disabled:opacity-40"
          >
            ×
          </button>
        </div>

        <div className="min-h-0 flex-1 overflow-y-auto p-5">
          {loading && !run && (
            <p className="py-8 text-center text-xs text-zinc-500">Loading planner…</p>
          )}

          {active && !composeNew && (
            <div className="rounded border border-status-running/30 bg-status-running/10 p-5 text-center">
              <span className="mx-auto mb-3 block h-2 w-2 animate-status-pulse rounded-full bg-status-running" />
              <p className="text-sm font-medium text-status-running">
                {run.status === "starting" ? "Starting planner…" : "Analyzing the Epic and repository…"}
              </p>
              <p className="mx-auto mt-2 max-w-lg text-[11px] leading-relaxed text-zinc-500">
                The generated tasks will appear here for review. Nothing is added to the board yet.
              </p>
              {run.session_id && (
                <button
                  onClick={() => void handleOpenTerminal()}
                  className="mt-4 rounded bg-surface-2 px-3 py-1.5 text-[10px] text-accent hover:bg-surface-3"
                >
                  Open planner terminal
                </button>
              )}
            </div>
          )}

          {ready && result && (
            <div>
              <div className="mb-4 rounded border border-accent/25 bg-accent/10 px-4 py-3">
                <div className="flex items-start gap-3">
                  <div className="min-w-0 flex-1">
                    <p className="text-xs font-medium text-zinc-200">
                      {result.tasks.length} proposed task{result.tasks.length === 1 ? "" : "s"}
                    </p>
                    <p className="mt-1 text-[10px] leading-relaxed text-zinc-500">
                      {result.summary}
                    </p>
                    {result.tasks.length > 0 && (
                      <p className="mt-1.5 text-[10px] leading-relaxed text-zinc-600">
                        Check the tasks you want to add. Required dependencies are selected automatically.
                      </p>
                    )}
                  </div>
                  <button
                    onClick={() => setComposeNew(true)}
                    className="whitespace-nowrap text-[10px] text-zinc-500 hover:text-accent"
                  >
                    Start over
                  </button>
                </div>
              </div>

              {result.tasks.length > 0 && (
                <div className="mb-2 flex items-center gap-2 px-1 text-[10px] text-zinc-500">
                  <span>{selectedCount} selected</span>
                  <button
                    onClick={() => setSelectedKeys(new Set(result.tasks.map((task) => task.key)))}
                    className="ml-auto hover:text-accent"
                  >
                    Select all
                  </button>
                  <button
                    onClick={() => setSelectedKeys(new Set())}
                    className="hover:text-zinc-300"
                  >
                    Clear
                  </button>
                </div>
              )}

              <div className="overflow-hidden rounded border border-surface-3 bg-surface-0/40">
                {result.tasks.map((task) => {
                  const checked = selectedKeys.has(task.key);
                  return (
                    <label
                      key={task.key}
                      className={`block cursor-pointer border-b border-surface-3 px-4 py-3 transition-colors last:border-b-0 ${
                        checked
                          ? "bg-accent/5"
                          : "hover:bg-surface-2/40"
                      }`}
                    >
                      <div className="flex items-start gap-3">
                        <input
                          type="checkbox"
                          checked={checked}
                          onChange={() => toggleTask(task.key)}
                          className="mt-0.5 accent-accent"
                        />
                        <div className="min-w-0 flex-1">
                          <div className="flex items-center gap-2">
                            <span className="rounded bg-surface-3 px-1.5 py-0.5 text-[9px] text-zinc-500">
                              {task.key}
                            </span>
                            <span className="text-xs font-medium text-zinc-200">{task.title}</span>
                          </div>
                          <p className="mt-1.5 whitespace-pre-wrap text-[10px] leading-relaxed text-zinc-500">
                            {task.description}
                          </p>
                          {task.acceptance_criteria.length > 0 && (
                            <ul className="mt-2 space-y-0.5 font-mono text-[10px] text-zinc-400">
                              {task.acceptance_criteria.map((criterion, index) => (
                                <li key={`${task.key}-${index}`} className="flex gap-1.5">
                                  <span className="text-zinc-600">-</span>
                                  <span>{criterion}</span>
                                </li>
                              ))}
                            </ul>
                          )}
                          {task.depends_on.length > 0 && (
                            <p className="mt-2 text-[9px] text-amber-400/80">
                              Depends on {task.depends_on.join(", ")}
                            </p>
                          )}
                        </div>
                      </div>
                    </label>
                  );
                })}
                {result.tasks.length === 0 && (
                  <div className="px-4 py-6 text-center text-xs text-zinc-500">
                    No tasks were generated. The link may require authentication; paste the Epic text and try again.
                  </div>
                )}
              </div>
            </div>
          )}

          {showComposer && (
            <form onSubmit={handleGenerate} className="space-y-4">
              {run?.repo_id === repoId && run.status === "failed" && !composeNew && (
                <div className="rounded border border-red-400/25 bg-red-400/10 px-3 py-2 text-[10px] leading-relaxed text-red-400">
                  {run.error ?? "The planner did not return a valid task plan."}
                </div>
              )}
              <label className="block">
                <span className="mb-1.5 block text-[10px] uppercase tracking-wider text-zinc-500">
                  Epic link or description
                </span>
                <textarea
                  value={sourceInput}
                  onChange={(event) => setSourceInput(event.target.value)}
                  placeholder="https://…/epic/123, or paste the complete feature description here…"
                  rows={12}
                  autoFocus
                  className="w-full resize-y rounded border border-surface-3 bg-surface-2 px-3 py-2.5 text-xs leading-relaxed text-zinc-200 placeholder-zinc-600 outline-none focus:border-accent"
                />
              </label>
              <label className="block">
                <span className="mb-1.5 block text-[10px] uppercase tracking-wider text-zinc-500">
                  Planner agent
                </span>
                <select
                  value={agent}
                  onChange={(event) => setAgent(event.target.value as "claude-code" | "codex")}
                  className="w-full rounded border border-surface-3 bg-surface-2 px-3 py-2 text-xs text-zinc-200 outline-none focus:border-accent"
                >
                  <option value="claude-code">Claude Code</option>
                  <option value="codex">Codex</option>
                </select>
              </label>
              <p className="text-[10px] leading-relaxed text-zinc-600">
                The planner runs read-only in the selected repository. Generated tasks are not created until you confirm the preview.
              </p>
              <div className="flex justify-end gap-2">
                <button
                  type="button"
                  onClick={onClose}
                  className="rounded px-3 py-2 text-xs text-zinc-500 hover:text-zinc-300"
                >
                  Cancel
                </button>
                <button
                  type="submit"
                  disabled={starting || !sourceInput.trim()}
                  className="rounded bg-accent px-4 py-2 text-xs font-medium text-white hover:bg-accent-hover disabled:cursor-not-allowed disabled:opacity-40"
                >
                  {starting ? "Starting planner…" : "Generate preview"}
                </button>
              </div>
            </form>
          )}

          {error && run?.status !== "failed" && (
            <p className="mt-3 rounded bg-red-500/10 px-3 py-2 text-[10px] text-red-400">
              {error}
            </p>
          )}
        </div>

        {ready && result && result.tasks.length > 0 && (
          <div className="flex items-center justify-between border-t border-surface-3 bg-surface-1 px-5 py-3">
            <span className="text-[10px] text-zinc-500">
              Only checked tasks will be added to Open.
            </span>
            <button
              onClick={() => void handleConfirm()}
              disabled={confirming || selectedCount === 0}
              className="rounded bg-accent px-4 py-2 text-xs font-medium text-white hover:bg-accent-hover disabled:cursor-not-allowed disabled:opacity-40"
            >
              {confirming ? "Creating tasks…" : `Create ${selectedCount} task${selectedCount === 1 ? "" : "s"}`}
            </button>
          </div>
        )}
      </div>
    </div>
  );
}
