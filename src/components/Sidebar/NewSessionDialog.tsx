import { useState } from "react";
import { useSessionStore } from "../../stores/sessionStore";

interface Props {
  open: boolean;
  onClose: () => void;
}

const AGENTS = [
  { id: "claude-code", label: "Claude Code" },
  { id: "aider", label: "Aider" },
  { id: "codex", label: "Codex" },
  { id: "shell", label: "Shell (bash)" },
];

export function NewSessionDialog({ open, onClose }: Props) {
  const [repoPath, setRepoPath] = useState("");
  const [project, setProject] = useState("");
  const [branch, setBranch] = useState("");
  const [agent, setAgent] = useState("claude-code");
  const [error, setError] = useState<string | null>(null);
  const [creating, setCreating] = useState(false);
  const createSession = useSessionStore((s) => s.createSession);

  if (!open) return null;

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!repoPath.trim() || !project.trim() || !branch.trim()) return;

    setCreating(true);
    setError(null);
    try {
      await createSession(repoPath.trim(), project.trim(), branch.trim(), agent);
      setRepoPath("");
      setProject("");
      setBranch("");
      setAgent("claude-code");
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
        className="w-96 rounded-lg border border-surface-3 bg-surface-1 p-6 shadow-2xl"
      >
        <h2 className="mb-4 text-sm font-semibold text-zinc-200">
          New Session
        </h2>

        <label className="mb-3 block">
          <span className="mb-1 block text-xs text-zinc-400">
            Repository path
          </span>
          <input
            type="text"
            value={repoPath}
            onChange={(e) => setRepoPath(e.target.value)}
            placeholder="/Users/you/projects/my-repo"
            autoFocus
            className="w-full rounded border border-surface-3 bg-surface-2 px-3 py-1.5 text-sm text-white placeholder-zinc-600 outline-none focus:border-accent"
          />
        </label>

        <label className="mb-3 block">
          <span className="mb-1 block text-xs text-zinc-400">
            Project name
          </span>
          <input
            type="text"
            value={project}
            onChange={(e) => setProject(e.target.value)}
            placeholder="my-app"
            className="w-full rounded border border-surface-3 bg-surface-2 px-3 py-1.5 text-sm text-white placeholder-zinc-600 outline-none focus:border-accent"
          />
        </label>

        <label className="mb-3 block">
          <span className="mb-1 block text-xs text-zinc-400">Branch</span>
          <input
            type="text"
            value={branch}
            onChange={(e) => setBranch(e.target.value)}
            placeholder="feat/new-feature"
            className="w-full rounded border border-surface-3 bg-surface-2 px-3 py-1.5 text-sm text-white placeholder-zinc-600 outline-none focus:border-accent"
          />
        </label>

        <label className="mb-4 block">
          <span className="mb-1 block text-xs text-zinc-400">Agent</span>
          <select
            value={agent}
            onChange={(e) => setAgent(e.target.value)}
            className="w-full rounded border border-surface-3 bg-surface-2 px-3 py-1.5 text-sm text-white outline-none focus:border-accent"
          >
            {AGENTS.map((a) => (
              <option key={a.id} value={a.id}>
                {a.label}
              </option>
            ))}
          </select>
        </label>

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
            disabled={creating || !repoPath.trim() || !project.trim() || !branch.trim()}
            className="rounded bg-accent px-3 py-1.5 text-xs font-medium text-white hover:bg-accent-hover disabled:opacity-50"
          >
            {creating ? "Creating..." : "Create"}
          </button>
        </div>
      </form>
    </div>
  );
}
