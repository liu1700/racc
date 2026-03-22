import { useState, useMemo, useEffect } from "react";
import type { Task } from "../../types/task";
import { useTaskStore } from "../../stores/taskStore";
import { useServerStore } from "../../stores/serverStore";

interface Props {
  task: Task;
  open: boolean;
  onClose: () => void;
}

function generateBranchName(description: string): string {
  return (
    "task/" +
    description
      .toLowerCase()
      .replace(/[^a-z0-9\s]/g, "")
      .trim()
      .split(/\s+/)
      .slice(0, 4)
      .join("-")
  );
}

export function FireTaskDialog({ task, open, onClose }: Props) {
  const [useWorktree, setUseWorktree] = useState(true);
  const [skipPermissions, setSkipPermissions] = useState(true);
  const defaultBranch = useMemo(
    () => generateBranchName(task.description),
    [task.description]
  );
  const [branch, setBranch] = useState(defaultBranch);
  const [serverId, setServerId] = useState<string>("");
  const [firing, setFiring] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const fireTask = useTaskStore((s) => s.fireTask);
  const servers = useServerStore((s) => s.servers);
  const loadServers = useServerStore((s) => s.loadServers);

  useEffect(() => {
    if (open) loadServers();
  }, [open, loadServers]);

  if (!open) return null;

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (useWorktree && !branch.trim()) return;

    setFiring(true);
    setError(null);
    try {
      await fireTask(
        task.id,
        task.repo_id,
        useWorktree,
        useWorktree ? branch.trim() : undefined,
        skipPermissions,
        serverId || undefined
      );
      onClose();
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setFiring(false);
    }
  };

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === "Escape") onClose();
  };

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/60"
      onKeyDown={handleKeyDown}
      onClick={onClose}
      tabIndex={-1}
      role="dialog"
      aria-modal="true"
      aria-labelledby="fire-task-title"
    >
      <form
        onSubmit={handleSubmit}
        onClick={(e) => e.stopPropagation()}
        className="w-80 rounded-lg border border-surface-3 bg-surface-1 p-5 shadow-2xl"
      >
        <h2 id="fire-task-title" className="mb-4 text-sm font-semibold text-zinc-200">
          Fire Task
        </h2>

        <div className="mb-4 max-h-40 overflow-y-auto rounded border-l-2 border-accent bg-surface-2 px-3 py-2 text-xs text-zinc-400 whitespace-pre-wrap">
          {task.description}
        </div>

        <label className="mb-3 block">
          <span className="mb-1 block text-xs text-zinc-400">Agent</span>
          <select className="w-full rounded border border-surface-3 bg-surface-2 px-3 py-1.5 text-sm text-white outline-none focus:border-accent">
            <option value="claude-code">Claude Code</option>
          </select>
        </label>

        <label className="mb-3 block">
          <span className="mb-1 block text-xs text-zinc-400">Run on</span>
          <select
            value={serverId}
            onChange={(e) => setServerId(e.target.value)}
            className="w-full rounded border border-surface-3 bg-surface-2 px-3 py-1.5 text-sm text-white outline-none focus:border-accent"
          >
            <option value="">Local machine</option>
            {servers.map((s) => (
              <option key={s.id} value={s.id}>{s.name}</option>
            ))}
          </select>
        </label>

        <label className="mb-4 flex items-center gap-2 text-xs text-zinc-300">
          <input
            type="checkbox"
            checked={skipPermissions}
            onChange={(e) => setSkipPermissions(e.target.checked)}
            className="accent-accent"
          />
          Skip permissions
        </label>

        <label className="mb-4 flex items-center gap-2 text-xs text-zinc-300">
          <input
            type="checkbox"
            checked={useWorktree}
            onChange={(e) => setUseWorktree(e.target.checked)}
            className="accent-accent"
          />
          Create a new worktree
        </label>

        {useWorktree && (
          <label className="mb-4 block">
            <span className="mb-1 block text-xs text-zinc-400">
              Branch name
            </span>
            <input
              type="text"
              value={branch}
              onChange={(e) => setBranch(e.target.value)}
              placeholder="task/my-feature"
              className="w-full rounded border border-surface-3 bg-surface-2 px-3 py-1.5 text-sm text-white placeholder-zinc-600 outline-none focus:border-accent"
            />
          </label>
        )}

        {error && (
          <p className="mb-3 rounded bg-red-500/10 px-3 py-2 text-xs text-red-400">
            {error}
          </p>
        )}

        <div className="flex justify-end gap-2">
          <button
            type="button"
            onClick={onClose}
            className="rounded px-3 py-1.5 text-xs text-zinc-400 hover:text-zinc-200"
          >
            Cancel
          </button>
          <button
            type="submit"
            disabled={firing || (useWorktree && !branch.trim())}
            className="rounded bg-accent px-3 py-1.5 text-xs font-medium text-white hover:bg-accent-hover disabled:opacity-50"
          >
            {firing ? "Firing..." : "Fire"}
          </button>
        </div>
      </form>
    </div>
  );
}
