import { useSessionStore } from "../../stores/sessionStore";
import type { SessionStatus } from "../../types/session";

export function StatusBar() {
  const repos = useSessionStore((s) => s.repos);
  const allSessions = repos.flatMap((r) => r.sessions);

  const counts: Record<SessionStatus, number> = {
    Running: 0,
    Completed: 0,
    Disconnected: 0,
    Error: 0,
  };
  for (const s of allSessions) {
    counts[s.status]++;
  }

  // Build categorical summary segments — only show non-zero categories
  const segments: { label: string; count: number; colorClass: string }[] = [];
  if (counts.Error > 0)
    segments.push({ label: "error", count: counts.Error, colorClass: "text-status-error" });
  if (counts.Running > 0)
    segments.push({ label: "running", count: counts.Running, colorClass: "text-status-running" });
  if (counts.Disconnected > 0)
    segments.push({ label: "disconnected", count: counts.Disconnected, colorClass: "text-status-disconnected" });
  if (counts.Completed > 0)
    segments.push({ label: "completed", count: counts.Completed, colorClass: "text-status-completed" });

  return (
    <footer className="flex items-center justify-between border-t border-surface-3 bg-surface-1 px-4 py-1.5 text-xs text-zinc-500">
      <div className="flex items-center gap-4">
        <span>
          Sessions:{" "}
          {segments.length === 0 ? (
            <span className="text-zinc-400">0</span>
          ) : (
            segments.map((seg, i) => (
              <span key={seg.label}>
                {i > 0 && <span className="mx-1 text-zinc-600">&middot;</span>}
                <span className={seg.colorClass}>{seg.count}</span>{" "}
                <span className="text-zinc-400">{seg.label}</span>
              </span>
            ))
          )}
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
