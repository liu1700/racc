import { useEffect, useMemo, useState } from "react";
import { useSessionStore } from "../../stores/sessionStore";
import { useTestManagerStore } from "../../stores/testManagerStore";
import type { TestResult, TestSettings } from "../../types/testManager";

interface Props {
  repoId: number;
  onSessionSelect?: () => void;
}

function parseResult(resultJson: string | null): Partial<TestResult> | null {
  if (!resultJson) return null;
  try {
    return JSON.parse(resultJson) as Partial<TestResult>;
  } catch {
    return null;
  }
}

const STATUS_STYLE: Record<string, string> = {
  starting: "text-accent",
  testing: "text-status-running",
  failed: "text-red-400",
  needs_review: "text-amber-400",
  succeeded: "text-status-completed",
};

export function TestManagerColumn({ repoId, onSessionSelect }: Props) {
  const {
    settings,
    activeRun,
    lastRun,
    loading,
    saving,
    starting,
    error,
    saveSettings,
    startRun,
    resolveRun,
    retryRun,
  } = useTestManagerStore();
  const openSession = useSessionStore((state) => state.openSession);
  const [draft, setDraft] = useState<TestSettings | null>(null);

  useEffect(() => {
    if (settings?.repo_id === repoId) setDraft(settings);
  }, [repoId, settings]);

  const result = useMemo(
    () => parseResult(lastRun?.result_json ?? null),
    [lastRun?.result_json],
  );
  const passedCount = result?.tests?.filter((test) => test.status === "passed").length ?? 0;
  const failedCount = result?.tests?.filter((test) => test.status === "failed").length ?? 0;
  const canStart = activeRun === null && !starting && !saving && draft !== null;

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

  const handleStart = async () => {
    await persistDraft();
    await startRun();
  };

  return (
    <section className="flex min-w-0 flex-col overflow-hidden rounded border border-surface-3 bg-surface-1/40">
      <div className="flex items-center gap-2 border-b border-surface-3 px-3 py-2">
        <span className="h-1.5 w-1.5 rounded-full bg-sky-400" />
        <span className="text-[10px] uppercase tracking-wider text-zinc-400">
          Test Manager
        </span>
        {(activeRun || starting) && (
          <span className="ml-auto text-[9px] text-status-running">Testing</span>
        )}
      </div>

      <div className="flex min-h-0 flex-1 flex-col gap-3 overflow-y-auto p-2.5">
        {loading && !draft && (
          <p className="px-1 text-[10px] text-zinc-600">Loading manager…</p>
        )}

        {!activeRun && !lastRun && !loading && (
          <div className="rounded border border-dashed border-surface-3 px-3 py-4 text-center text-[10px] leading-relaxed text-zinc-600">
            Run a full-project UAT pass in an isolated worktree.
          </div>
        )}

        {activeRun && (
          <button
            onClick={() => void handleOpenRun()}
            className="rounded border border-status-running/30 bg-status-running/10 px-2.5 py-2 text-left"
          >
            <span className="block text-[10px] font-medium text-status-running">
              {activeRun.worktree_branch ?? "Test Manager"}
            </span>
            <span className="mt-0.5 block text-[9px] text-zinc-500">
              Open the Test Manager terminal →
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
                    agent: event.target.value as TestSettings["agent"],
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
                Test instructions
              </span>
              <textarea
                value={draft.instructions}
                onChange={(event) => setDraft({ ...draft, instructions: event.target.value })}
                onBlur={() => void persistDraft()}
                rows={8}
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
              {result?.tests && (
                <span className="text-[9px] text-zinc-500">
                  {passedCount} passed{failedCount ? ` · ${failedCount} failed` : ""}
                </span>
              )}
              {lastRun.session_id && (
                <button onClick={() => void handleOpenRun()} className="ml-auto text-accent hover:underline">
                  Terminal
                </button>
              )}
            </div>
            {result?.summary && (
              <p className="mt-1 text-[9px] leading-relaxed text-zinc-500">
                {result.summary}
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
                  disabled={starting}
                  className="rounded bg-accent/15 px-2 py-1 text-[9px] text-accent disabled:opacity-50"
                >
                  Retry
                </button>
              </div>
            )}
            {lastRun.status === "failed" && (
              <button
                onClick={() => void retryRun()}
                disabled={starting}
                className="mt-2 rounded bg-accent/15 px-2 py-1 text-[9px] text-accent disabled:opacity-50"
              >
                Retry test run
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
          onClick={() => void handleStart()}
          disabled={!canStart}
          className="w-full rounded bg-accent px-3 py-2 text-[11px] font-medium text-white transition-colors hover:bg-accent-hover disabled:cursor-not-allowed disabled:opacity-40"
        >
          {starting ? "Starting Test Manager…" : "Run"}
        </button>
      </div>
    </section>
  );
}
