import { useEffect } from "react";
import { useAssistantStore } from "../../stores/assistantStore";
import { useShallow } from "zustand/react/shallow";
import { AssistantSetup } from "./AssistantSetup";
import { AssistantChat } from "./AssistantChat";

export function AssistantPanel() {
  const { config, assistantCost, loadConfig, loadHistory } = useAssistantStore(
    useShallow((s) => ({
      config: s.config,
      assistantCost: s.assistantCost,
      loadConfig: s.loadConfig,
      loadHistory: s.loadHistory,
    }))
  );

  useEffect(() => {
    loadConfig();
    loadHistory();
  }, [loadConfig, loadHistory]);

  const isConfigured = config?.api_key && config?.model;

  return (
    <div className="flex flex-1 flex-col overflow-hidden border-t border-surface-3">
      <div className="flex items-center justify-between border-b border-surface-3 px-4 py-2">
        <h2 className="text-xs font-semibold uppercase tracking-wider text-zinc-400">
          Assistant
        </h2>
        {isConfigured && assistantCost > 0 && (
          <span className="text-[10px] text-zinc-600">
            ${assistantCost.toFixed(4)}
          </span>
        )}
      </div>

      {isConfigured ? <AssistantChat /> : <AssistantSetup />}
    </div>
  );
}
