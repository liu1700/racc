import { useState } from "react";
import { useSessionStore } from "../../stores/sessionStore";
import type { Session } from "../../types/session";

interface Props {
  session: Session;
  open: boolean;
  onClose: () => void;
}

export function RemoveSessionDialog({ session, open, onClose }: Props) {
  const [removeWorktree, setRemoveWorktree] = useState(false);
  const [removing, setRemoving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const removeSession = useSessionStore((s) => s.removeSession);

  if (!open) return null;

  const hasWorktree = !!session.worktree_path;

  const handleConfirm = async () => {
    setRemoving(true);
    setError(null);
    try {
      await removeSession(session.id, hasWorktree && removeWorktree);
      onClose();
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setRemoving(false);
    }
  };

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/60"
      onKeyDown={(e) => e.key === "Escape" && onClose()}
    >
      <div className="w-80 rounded-lg border border-surface-3 bg-surface-1 p-5 shadow-2xl">
        <h2 className="mb-3 text-sm font-semibold text-zinc-200">
          Remove Session
        </h2>
        <p className="mb-4 text-xs text-zinc-400">
          Are you sure you want to remove this session
          {session.branch ? ` (${session.branch})` : ""}?
        </p>

        {hasWorktree && (
          <label className="mb-4 flex items-center gap-2 text-xs text-zinc-300">
            <input
              type="checkbox"
              checked={removeWorktree}
              onChange={(e) => setRemoveWorktree(e.target.checked)}
              className="accent-accent"
            />
            Also remove worktree folder
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
            type="button"
            onClick={handleConfirm}
            disabled={removing}
            className="rounded bg-red-600 px-3 py-1.5 text-xs font-medium text-white hover:bg-red-500 disabled:opacity-50"
          >
            {removing ? "Removing..." : "Remove"}
          </button>
        </div>
      </div>
    </div>
  );
}
