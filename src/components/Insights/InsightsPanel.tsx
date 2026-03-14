import { useEffect, useState } from "react";
import { useInsightsStore } from "../../stores/insightsStore";
import { useAssistantStore } from "../../stores/assistantStore";
import { InsightCard } from "./InsightCard";
import { AssistantSetup } from "../Assistant/AssistantSetup";

function timeAgo(timestamp: number): string {
  const seconds = Math.floor((Date.now() - timestamp) / 1000);
  if (seconds < 60) return "Just now";
  const minutes = Math.floor(seconds / 60);
  if (minutes < 60) return `${minutes}m ago`;
  const hours = Math.floor(minutes / 60);
  if (hours < 24) return `${hours}h ago`;
  const days = Math.floor(hours / 24);
  return `${days}d ago`;
}

const SEVERITY_DOT_COLOR: Record<string, string> = {
  alert: "bg-status-error",
  warning: "bg-yellow-500",
  suggestion: "bg-status-completed",
};

export function InsightsPanel() {
  const insights = useInsightsStore((s) => s.insights);
  const expandedId = useInsightsStore((s) => s.expandedId);
  const loading = useInsightsStore((s) => s.loading);
  const initialize = useInsightsStore((s) => s.initialize);
  const toggleExpand = useInsightsStore((s) => s.toggleExpand);
  const dismissInsight = useInsightsStore((s) => s.dismissInsight);
  const applyInsight = useInsightsStore((s) => s.applyInsight);
  const config = useAssistantStore((s) => s.config);
  const loadConfig = useAssistantStore((s) => s.loadConfig);
  const [showSettings, setShowSettings] = useState(false);
  const [configLoaded, setConfigLoaded] = useState(false);

  useEffect(() => {
    initialize();
    loadConfig().then(() => setConfigLoaded(true));
  }, [initialize, loadConfig]);

  const needsSetup = configLoaded && (!config || !config.api_key);

  if (showSettings || needsSetup) {
    return <AssistantSetup onBack={needsSetup ? undefined : () => setShowSettings(false)} />;
  }

  return (
    <div className="flex h-full flex-col">
      <div className="flex items-center justify-between border-b border-surface-3 px-4 py-2.5">
        <span className="text-sm font-semibold text-text-primary">Insights</span>
        <div className="flex items-center gap-2">
          {insights.length > 0 && (
            <span className="rounded-full bg-accent/20 px-2 py-0.5 text-[10px] font-medium text-accent">
              {insights.length} active
            </span>
          )}
          <button
            onClick={() => setShowSettings(true)}
            className="text-text-tertiary hover:text-text-secondary"
            title="API settings (for LLM-generated suggestions)"
          >
            <span className="text-sm">{"\u2699"}</span>
          </button>
        </div>
      </div>

      <div className="flex-1 overflow-y-auto px-3 py-3">
        {loading ? (
          <div className="flex items-center justify-center py-12">
            <span className="text-xs text-text-tertiary">Loading...</span>
          </div>
        ) : insights.length === 0 ? (
          <div className="flex flex-col items-center justify-center py-16 text-center">
            <div className="mb-3 text-2xl opacity-30">{"\u25C7"}</div>
            <div className="text-xs text-text-tertiary">
              No insights yet. Patterns will appear
              <br />
              as you work across sessions.
            </div>
          </div>
        ) : (
          <div className="relative">
            <div className="absolute left-[5px] top-2 bottom-2 w-px bg-surface-3" />

            <div className="space-y-3">
              {insights.map((insight) => (
                <div key={insight.id} className="relative pl-5">
                  <div
                    className={`absolute left-0 top-3 h-[10px] w-[10px] rounded-full border-2 border-surface-0 ${
                      SEVERITY_DOT_COLOR[insight.severity] || "bg-surface-3"
                    }`}
                  />
                  <div className="mb-1 text-[9px] text-text-tertiary">
                    {timeAgo(insight.created_at)}
                  </div>
                  <InsightCard
                    insight={insight}
                    expanded={expandedId === insight.id}
                    onToggle={() => toggleExpand(insight.id)}
                    onDismiss={() => dismissInsight(insight.id)}
                    onApply={() => applyInsight(insight.id)}
                  />
                </div>
              ))}
            </div>
          </div>
        )}
      </div>
    </div>
  );
}
