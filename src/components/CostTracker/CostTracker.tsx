import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useSessionStore } from "../../stores/sessionStore";
import type { ProjectCosts } from "../../types/cost";

const COST_POLL_INTERVAL_MS = 10_000;

function formatTokens(n: number): string {
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`;
  if (n >= 1_000) return `${(n / 1_000).toFixed(1)}k`;
  return n.toString();
}

export function CostTracker() {
  const [costs, setCosts] = useState<ProjectCosts | null>(null);
  const active = useSessionStore((s) => s.getActiveSession());
  const worktreePath = active?.session.worktree_path ?? active?.repo.path;

  useEffect(() => {
    if (!worktreePath) {
      setCosts(null);
      return;
    }

    let cancelled = false;

    const fetchCosts = async () => {
      try {
        const data = await invoke<ProjectCosts>("get_project_costs", {
          worktreePath,
        });
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
  }, [worktreePath]);

  return (
    <div className="border-b border-surface-3 bg-surface-1 px-4 py-3">
      <h2 className="text-xs font-semibold uppercase tracking-wider text-zinc-400">
        Cost
      </h2>
      <div className="mt-2 grid grid-cols-2 gap-3">
        <div>
          <p className="text-xs text-zinc-500">Total cost</p>
          <p className="text-lg font-semibold text-white">
            ${costs?.total_estimated_cost_usd.toFixed(2) ?? "0.00"}
          </p>
        </div>
        <div>
          <p className="text-xs text-zinc-500">Sessions</p>
          <p className="text-lg font-semibold text-white">
            {costs?.sessions.length ?? 0}
          </p>
        </div>
        <div>
          <p className="text-xs text-zinc-500">Input tokens</p>
          <p className="text-sm text-zinc-300">
            {formatTokens(costs?.total_input_tokens ?? 0)}
          </p>
        </div>
        <div>
          <p className="text-xs text-zinc-500">Output tokens</p>
          <p className="text-sm text-zinc-300">
            {formatTokens(costs?.total_output_tokens ?? 0)}
          </p>
        </div>
        <div>
          <p className="text-xs text-zinc-500">Cache write</p>
          <p className="text-sm text-zinc-300">
            {formatTokens(costs?.total_cache_creation_tokens ?? 0)}
          </p>
        </div>
        <div>
          <p className="text-xs text-zinc-500">Cache read</p>
          <p className="text-sm text-zinc-300">
            {formatTokens(costs?.total_cache_read_tokens ?? 0)}
          </p>
        </div>
      </div>
    </div>
  );
}
