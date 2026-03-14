import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useSessionStore } from "../../stores/sessionStore";
import type { SessionStatus } from "../../types/session";
import type { ProjectCosts } from "../../types/cost";

function formatTokens(n: number): string {
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`;
  if (n >= 1_000) return `${(n / 1_000).toFixed(1)}k`;
  return n.toString();
}

const COST_POLL_INTERVAL_MS = 10_000;

export function StatusBar() {
  const repos = useSessionStore((s) => s.repos);
  const allSessions = repos.flatMap((r) => r.sessions);
  const [costs, setCosts] = useState<ProjectCosts | null>(null);

  useEffect(() => {
    let cancelled = false;

    const fetchCosts = async () => {
      try {
        const data = await invoke<ProjectCosts>("get_global_costs");
        if (!cancelled) setCosts(data);
      } catch {
        // Silent fail — cost tracking is non-critical
      }
    };

    fetchCosts();
    const interval = setInterval(fetchCosts, COST_POLL_INTERVAL_MS);
    return () => {
      cancelled = true;
      clearInterval(interval);
    };
  }, []);

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

  const totalTokens = (costs?.total_input_tokens ?? 0) + (costs?.total_output_tokens ?? 0);

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
          Total Tokens: <span className="text-zinc-300">{formatTokens(totalTokens)}</span>
        </span>
        <span>
          This Week:{" "}
          <span className="text-zinc-300">
            {formatTokens((costs?.week_input_tokens ?? 0) + (costs?.week_output_tokens ?? 0))}
          </span>
        </span>
      </div>
    </footer>
  );
}
