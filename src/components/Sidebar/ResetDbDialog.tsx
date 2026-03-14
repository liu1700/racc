import { useState } from "react";
import { useSessionStore } from "../../stores/sessionStore";

interface Props {
  open: boolean;
  onClose: () => void;
}

export function ResetDbDialog({ open, onClose }: Props) {
  const [resetting, setResetting] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const resetDb = useSessionStore((s) => s.resetDb);

  if (!open) return null;

  const handleConfirm = async () => {
    setResetting(true);
    setError(null);
    try {
      await resetDb();
      onClose();
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setResetting(false);
    }
  };

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/60"
      onKeyDown={(e) => e.key === "Escape" && onClose()}
    >
      <div className="w-80 rounded-lg border border-surface-3 bg-surface-1 p-5 shadow-2xl">
        <h2 className="mb-3 text-sm font-semibold text-zinc-200">
          Reset Database
        </h2>
        <p className="mb-4 text-xs text-zinc-400">
          This will delete all local data including repos, sessions, and
          assistant chat history. This action cannot be undone.
        </p>

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
            disabled={resetting}
            className="rounded bg-red-600 px-3 py-1.5 text-xs font-medium text-white hover:bg-red-500 disabled:opacity-50"
          >
            {resetting ? "Resetting..." : "Reset"}
          </button>
        </div>
      </div>
    </div>
  );
}
