import type { Insight } from "../../types/insights";
import { InsightActions } from "./InsightActions";

const SEVERITY_STYLES: Record<string, { border: string; icon: string; iconColor: string }> = {
  alert: { border: "border-l-status-error", icon: "\u26A0", iconColor: "text-status-error" },
  warning: { border: "border-l-yellow-500", icon: "\u25C7", iconColor: "text-yellow-500" },
  suggestion: { border: "border-l-status-completed", icon: "\u2726", iconColor: "text-status-completed" },
};

interface InsightCardProps {
  insight: Insight;
  expanded: boolean;
  onToggle: () => void;
  onDismiss: () => void;
  onApply: () => void;
}

export function InsightCard({ insight, expanded, onToggle, onDismiss, onApply }: InsightCardProps) {
  const style = SEVERITY_STYLES[insight.severity] || SEVERITY_STYLES.warning;
  let detail: Record<string, unknown> = {};
  try {
    detail = JSON.parse(insight.detail_json);
  } catch { /* ignore */ }

  return (
    <div
      className={`rounded-md border bg-surface-1 transition-all ${
        expanded ? "border-accent" : `border-surface-3 ${style.border} border-l-2`
      }`}
    >
      <button
        onClick={onToggle}
        className="flex w-full items-center gap-2 px-3 py-2.5 text-left"
      >
        <span className={`text-sm ${style.iconColor}`}>{style.icon}</span>
        <div className="min-w-0 flex-1">
          <div className="truncate text-xs font-medium text-text-primary">{insight.title}</div>
          <div className="truncate text-[11px] text-text-secondary">{insight.summary}</div>
        </div>
        {expanded && (
          <span className="text-[10px] text-text-tertiary">{"\u25B2"}</span>
        )}
      </button>

      {expanded && (
        <div className="border-t border-surface-3 bg-surface-0 px-3 py-3">
          <div className="mb-3">
            <div className="mb-1.5 text-[10px] font-medium uppercase tracking-wider text-text-tertiary">
              Evidence
            </div>
            <EvidenceList insightType={insight.insight_type} detail={detail} />
          </div>

          {detail.suggestedEntry != null && (
            <div className="mb-3">
              <div className="mb-1.5 text-[10px] font-medium uppercase tracking-wider text-text-tertiary">
                Suggested
              </div>
              <div className="rounded bg-surface-1 border border-surface-3 px-2.5 py-2 font-mono text-[11px] text-status-completed">
                {String(detail.suggestedEntry)}
              </div>
            </div>
          )}

          <InsightActions
            insightType={insight.insight_type}
            detail={detail}
            onApply={onApply}
            onDismiss={onDismiss}
          />
        </div>
      )}
    </div>
  );
}

function EvidenceList({ insightType, detail }: { insightType: string; detail: Record<string, unknown> }) {
  switch (insightType) {
    case "repeated_prompt": {
      const matches = (detail.matches as Array<{ sessionId: number; text: string; timestamp: number }>) || [];
      return (
        <div className="space-y-1">
          {matches.slice(0, 5).map((m, i) => (
            <div key={i} className="rounded bg-surface-1 border border-surface-3 px-2 py-1.5">
              <div className="flex items-center justify-between">
                <span className="text-[10px] text-accent">session-{m.sessionId}</span>
                <span className="text-[9px] text-text-tertiary">
                  {new Date(m.timestamp).toLocaleTimeString()}
                </span>
              </div>
              <div className="mt-0.5 text-[10px] italic text-text-secondary">&quot;{m.text}&quot;</div>
            </div>
          ))}
          {matches.length > 5 && (
            <div className="text-center text-[9px] text-text-tertiary">
              + {matches.length - 5} more
            </div>
          )}
        </div>
      );
    }

    case "file_conflict": {
      const sessions = (detail.sessions as Array<{ sessionId: number; operation: string }>) || [];
      return (
        <div className="space-y-1">
          <div className="text-[11px] font-mono text-text-primary">{String(detail.filePath)}</div>
          {sessions.map((s, i) => (
            <div key={i} className="text-[10px] text-text-secondary">
              session-{s.sessionId}: {s.operation}
            </div>
          ))}
        </div>
      );
    }

    case "cost_anomaly": {
      const d = detail as { currentCost?: number; averageCost?: number; sessionId?: number };
      return (
        <div className="text-[11px] text-text-secondary">
          <div>Session {d.sessionId}: ${(d.currentCost ?? 0).toFixed(2)} in last interval</div>
          <div>Average: ${(d.averageCost ?? 0).toFixed(2)}</div>
        </div>
      );
    }

    case "repeated_permission": {
      const d = detail as { permissionType?: string; count?: number; sessionId?: number };
      return (
        <div className="text-[11px] text-text-secondary">
          Permission &quot;{d.permissionType}&quot; requested {d.count} times in session {d.sessionId}
        </div>
      );
    }

    case "startup_pattern": {
      const commands = (detail.commands as string[]) || [];
      const sessions = (detail.sessions as Array<{ sessionId: number }>) || [];
      return (
        <div>
          <div className="mb-1 text-[10px] text-text-secondary">
            Found in {sessions.length} sessions:
          </div>
          <div className="space-y-0.5">
            {commands.map((cmd, i) => (
              <div key={i} className="rounded bg-surface-1 border border-surface-3 px-2 py-1 font-mono text-[10px] text-text-primary">
                {cmd}
              </div>
            ))}
          </div>
        </div>
      );
    }

    case "similar_sessions": {
      const d = detail as {
        sessionA?: { id: number };
        sessionB?: { id: number };
        similarity?: number;
        sharedFiles?: string[];
      };
      return (
        <div className="text-[11px] text-text-secondary">
          <div>Sessions {d.sessionA?.id} and {d.sessionB?.id}</div>
          <div>Similarity: {((d.similarity ?? 0) * 100).toFixed(0)}%</div>
          {d.sharedFiles && d.sharedFiles.length > 0 && (
            <div className="mt-1">
              <div className="text-[10px] text-text-tertiary">Shared files:</div>
              {d.sharedFiles.slice(0, 3).map((f, i) => (
                <div key={i} className="font-mono text-[10px]">{f}</div>
              ))}
            </div>
          )}
        </div>
      );
    }

    default:
      return <div className="text-[11px] text-text-tertiary">No detail available</div>;
  }
}
