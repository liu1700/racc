import { useState } from "react";
import { useSessionStore } from "../../stores/sessionStore";

interface Props {
  repoId: number;
  open: boolean;
  onClose: () => void;
}

export function NewAgentDialog({ repoId, open: isOpen, onClose }: Props) {
  const [useWorktree, setUseWorktree] = useState(false);
  const [branch, setBranch] = useState("");
  const [skipPermissions, setSkipPermissions] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [creating, setCreating] = useState(false);
  const createSession = useSessionStore((s) => s.createSession);

  if (!isOpen) return null;

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (useWorktree && !branch.trim()) return;

    setCreating(true);
    setError(null);
    try {
      await createSession(repoId, useWorktree, useWorktree ? branch.trim() : undefined, skipPermissions);
      setBranch("");
      setUseWorktree(false);
      onClose();
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setCreating(false);
    }
  };

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === "Escape") onClose();
  };

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/60"
      onKeyDown={handleKeyDown}
    >
      <form
        onSubmit={handleSubmit}
        className="w-80 rounded-lg border border-surface-3 bg-surface-1 p-5 shadow-2xl"
      >
        <h2 className="mb-4 text-sm font-semibold text-zinc-200">
          Launch Agent
        </h2>

        <label className="mb-3 block">
          <span className="mb-1 block text-xs text-zinc-400">Agent</span>
          <select
            className="w-full rounded border border-surface-3 bg-surface-2 px-3 py-1.5 text-sm text-white outline-none focus:border-accent"
            defaultValue="claude-code"
          >
            <option value="claude-code">Claude Code</option>
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
              placeholder="feat/my-feature"
              autoFocus
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
            disabled={creating || (useWorktree && !branch.trim())}
            className="rounded bg-accent px-3 py-1.5 text-xs font-medium text-white hover:bg-accent-hover disabled:opacity-50"
          >
            {creating ? "Launching..." : "Launch"}
          </button>
        </div>
      </form>
    </div>
  );
}
