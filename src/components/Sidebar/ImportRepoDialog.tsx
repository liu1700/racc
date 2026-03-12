import { useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { useSessionStore } from "../../stores/sessionStore";

export function ImportRepoDialog() {
  const [error, setError] = useState<string | null>(null);
  const [importing, setImporting] = useState(false);
  const importRepo = useSessionStore((s) => s.importRepo);

  const handleImport = async () => {
    setError(null);
    setImporting(true);
    try {
      const selected = await open({ directory: true, multiple: false });
      if (!selected) {
        setImporting(false);
        return;
      }
      await importRepo(selected);
    } catch (e) {
      setError(String(e));
    } finally {
      setImporting(false);
    }
  };

  return (
    <div>
      <button
        onClick={handleImport}
        disabled={importing}
        className="flex w-full items-center gap-2 rounded px-3 py-2 text-xs text-zinc-400 hover:bg-surface-2 hover:text-zinc-200 disabled:opacity-50"
      >
        <span className="text-base leading-none">+</span>
        {importing ? "Importing..." : "Import Repo"}
      </button>
      {error && (
        <p className="mx-3 mt-1 text-xs text-red-400">{error}</p>
      )}
    </div>
  );
}
