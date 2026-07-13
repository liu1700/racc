import { useEffect, useMemo, useState } from "react";
import { useMergeStore } from "../../stores/mergeStore";
import { useSessionStore } from "../../stores/sessionStore";
import { useTaskStore } from "../../stores/taskStore";
import { parsePrDisplay } from "../../utils/prUrl";
import { shipRunCanStart } from "../../utils/mergeManager";
import type { MergeSettings } from "../../types/merge";

interface Props {
  repoId: number;
  onSessionSelect?: () => void;
}

function resultSummary(resultJson: string | null): string | null {
  if (!resultJson) return null;
  try {
    const result = JSON.parse(resultJson) as { summary?: string };
    return result.summary ?? null;
  } catch {
    return null;
  }
}

const STATUS_STYLE: Record<string, string> = {
  queued: "text-accent",
  shipping: "text-status-running",
  failed: "text-red-400",
  needs_review: "text-amber-400",
  succeeded: "text-status-completed",
};

export function MergeManagerColumn({ repoId, onSessionSelect }: Props) {
  const {
    settings,
    items,
    activeRun,
    lastRun,
    loading,
    saving,
    shipping,
    error,
    saveSettings,
    setReady,
    startRun,
    resolveRun,
    retryRun,
  } = useMergeStore();
  const tasks = useTaskStore((state) => state.tasks);
  const openSession = useSessionStore((state) => state.openSession);
  const [draft, setDraft] = useState<MergeSettings | null>(null);

  useEffect(() => {
    if (settings?.repo_id === repoId) setDraft(settings);
  }, [repoId, settings]);

  const visibleItems = useMemo(
    () => items.filter((item) => item.status !== "succeeded"),
    [items],
  );
  const queuedCount = items.filter((item) => item.status === "queued").length;
  const canShip = shipRunCanStart(items, activeRun) && !shipping && !saving;

  const persistDraft = async (next = draft) => {
    if (!next) return;
    if (
      settings &&
      next.target_branch === settings.target_branch &&
      next.agent === settings.agent &&
      next.instructions === settings.instructions
    ) return;
    await saveSettings(next);
  };

  const handleOpenRun = async () => {
    const sessionId = activeRun?.session_id ?? lastRun?.session_id;
    if (!sessionId) return;
    await openSession(sessionId);
    onSessionSelect?.();
  };

  const handleShipAll = async () => {
    await persistDraft();
    await startRun();
  };

  return (
    <section className="flex min-w-0 flex-col overflow-hidden rounded border border-surface-3 bg-surface-1/40">
      <div className="flex items-center gap-2 border-b border-surface-3 px-3 py-2">
        <span className="h-1.5 w-1.5 rounded-full bg-amber-400" />
        <span className="text-[10px] uppercase tracking-wider text-zinc-400">
          Merge Manager
        </span>
        <span className="text-[10px] text-zinc-600">{queuedCount}</span>
        {(activeRun || shipping) && (
          <span className="ml-auto text-[9px] text-status-running">Shipping</span>
        )}
      </div>

      <div className="flex min-h-0 flex-1 flex-col gap-3 overflow-y-auto p-2.5">
        <div className="space-y-1.5">
          {loading && items.length === 0 && (
            <p className="px-1 text-[10px] text-zinc-600">Loading queue…</p>
          )}
          {!loading && visibleItems.length === 0 && (
            <div className="rounded border border-dashed border-surface-3 px-3 py-4 text-center text-[10px] leading-relaxed text-zinc-600">
              Mark a PR as Ready to merge from the Working column.
            </div>
          )}
          {visibleItems.map((item) => {
            const task = tasks.find((candidate) => candidate.id === item.task_id);
            const pr = parsePrDisplay(item.pr_url);
            return (
              <div key={item.id} className="rounded border border-surface-3 bg-surface-1 p-2">
                <div className="flex items-start gap-2">
                  <div className="min-w-0 flex-1">
                    <div className="flex items-center gap-1.5 text-[10px]">
                      <span className="font-medium text-zinc-300">{pr?.label ?? "PR"}</span>
                      <span className={`ml-auto capitalize ${STATUS_STYLE[item.status] ?? "text-zinc-500"}`}>
                        {item.status.replace("_", " ")}
                      </span>
                    </div>
                    <p className="mt-1 line-clamp-2 text-[10px] leading-snug text-zinc-500">
                      {task?.description ?? item.pr_url}
                    </p>
                    {item.result_message && (
                      <p className="mt-1 line-clamp-2 text-[9px] text-zinc-600">
                        {item.result_message}
                      </p>
                    )}
                  </div>
                  {item.status === "queued" && (
                    <button
                      onClick={() => void setReady(item.task_id, false)}
                      className="text-xs text-zinc-600 hover:text-red-400"
                      title="Remove from merge queue"
                    >
                      ×
                    </button>
                  )}
                </div>
              </div>
            );
          })}
        </div>

        {activeRun && (
          <button
            onClick={() => void handleOpenRun()}
            className="rounded border border-status-running/30 bg-status-running/10 px-2.5 py-2 text-left"
          >
            <span className="block text-[10px] font-medium text-status-running">
              {activeRun.integration_branch ?? "Merge Master"}
            </span>
            <span className="mt-0.5 block text-[9px] text-zinc-500">
              Open the Merge Master terminal →
            </span>
          </button>
        )}

        {draft && (
          <div className="space-y-2 border-t border-surface-3 pt-3">
            <label className="block">
              <span className="mb-1 block text-[9px] uppercase tracking-wider text-zinc-600">
                Target branch
              </span>
              <input
                value={draft.target_branch}
                onChange={(event) => setDraft({ ...draft, target_branch: event.target.value })}
                onBlur={() => void persistDraft()}
                className="w-full rounded border border-surface-3 bg-surface-2 px-2 py-1.5 text-[11px] text-zinc-300 outline-none focus:border-accent"
              />
            </label>

            <label className="block">
              <span className="mb-1 block text-[9px] uppercase tracking-wider text-zinc-600">
                Agent
              </span>
              <select
                value={draft.agent}
                onChange={(event) => {
                  const next = {
                    ...draft,
                    agent: event.target.value as MergeSettings["agent"],
                  };
                  setDraft(next);
                  void persistDraft(next);
                }}
                className="w-full rounded border border-surface-3 bg-surface-2 px-2 py-1.5 text-[11px] text-zinc-300 outline-none focus:border-accent"
              >
                <option value="claude-code">Claude Code</option>
                <option value="codex">Codex</option>
              </select>
            </label>

            <label className="block">
              <span className="mb-1 block text-[9px] uppercase tracking-wider text-zinc-600">
                Ship instructions
              </span>
              <textarea
                value={draft.instructions}
                onChange={(event) => setDraft({ ...draft, instructions: event.target.value })}
                onBlur={() => void persistDraft()}
                rows={6}
                className="w-full resize-y rounded border border-surface-3 bg-surface-2 px-2 py-1.5 text-[10px] leading-relaxed text-zinc-300 outline-none focus:border-accent"
              />
            </label>
            <p className="text-right text-[9px] text-zinc-600">
              {saving ? "Saving…" : "Saved per repository"}
            </p>
          </div>
        )}

        {lastRun && !activeRun && (
          <div className={`rounded border px-2.5 py-2 ${
            lastRun.status === "succeeded"
              ? "border-status-completed/30 bg-status-completed/10"
              : lastRun.status === "needs_review"
                ? "border-amber-400/30 bg-amber-400/10"
                : "border-red-400/30 bg-red-400/10"
          }`}>
            <div className="flex items-center gap-2 text-[10px]">
              <span className={`font-medium capitalize ${STATUS_STYLE[lastRun.status] ?? "text-zinc-400"}`}>
                {lastRun.status.replace("_", " ")}
              </span>
              {lastRun.session_id && (
                <button onClick={() => void handleOpenRun()} className="ml-auto text-accent hover:underline">
                  Terminal
                </button>
              )}
            </div>
            {resultSummary(lastRun.result_json) && (
              <p className="mt-1 text-[9px] leading-relaxed text-zinc-500">
                {resultSummary(lastRun.result_json)}
              </p>
            )}
            {lastRun.status === "needs_review" && (
              <div className="mt-2 flex flex-wrap gap-1">
                <button
                  onClick={() => void resolveRun("succeeded")}
                  className="rounded bg-status-completed/15 px-2 py-1 text-[9px] text-status-completed"
                >
                  Mark succeeded
                </button>
                <button
                  onClick={() => void resolveRun("failed")}
                  className="rounded bg-red-400/10 px-2 py-1 text-[9px] text-red-400"
                >
                  Mark failed
                </button>
                <button
                  onClick={() => void retryRun()}
                  disabled={shipping}
                  className="rounded bg-accent/15 px-2 py-1 text-[9px] text-accent disabled:opacity-50"
                >
                  Retry
                </button>
              </div>
            )}
            {lastRun.status === "failed" && items.some((item) => item.status === "failed" || item.status === "needs_review") && (
              <button
                onClick={() => void retryRun()}
                disabled={shipping}
                className="mt-2 rounded bg-accent/15 px-2 py-1 text-[9px] text-accent disabled:opacity-50"
              >
                Retry failed items
              </button>
            )}
          </div>
        )}

        {error && (
          <p className="rounded bg-red-500/10 px-2.5 py-2 text-[10px] leading-relaxed text-red-400">
            {error}
          </p>
        )}
      </div>

      <div className="border-t border-surface-3 bg-surface-1 p-2.5">
        <button
          onClick={() => void handleShipAll()}
          disabled={!canShip}
          className="w-full rounded bg-accent px-3 py-2 text-[11px] font-medium text-white transition-colors hover:bg-accent-hover disabled:cursor-not-allowed disabled:opacity-40"
        >
          {shipping ? "Starting Merge Master…" : `Ship All${queuedCount ? ` (${queuedCount})` : ""}`}
        </button>
      </div>
    </section>
  );
}
