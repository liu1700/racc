import { useSessionStore } from "../../stores/sessionStore";

export function StatusBar() {
  const sessions = useSessionStore((s) => s.sessions);
  const activeSessions = sessions.filter(
    (s) => s.status === "Running" || s.status === "Waiting",
  ).length;

  return (
    <footer className="flex items-center justify-between border-t border-surface-3 bg-surface-1 px-4 py-1.5 text-xs text-zinc-500">
      <div className="flex items-center gap-4">
        <span>
          Sessions:{" "}
          <span className="text-zinc-300">{activeSessions} active</span>
        </span>
        <span>
          Total Cost: <span className="text-zinc-300">$0.00</span>
        </span>
        <span>
          This Week: <span className="text-zinc-300">$0.00</span>
        </span>
      </div>
      <div className="flex items-center gap-2">
        <span className="h-1.5 w-1.5 rounded-full bg-status-running" />
        <span>Connected</span>
      </div>
    </footer>
  );
}
