import { useState } from "react";
import { useAssistantStore } from "../../stores/assistantStore";
import { useShallow } from "zustand/react/shallow";

interface AssistantSetupProps {
  onBack?: () => void;
}

export function AssistantSetup({ onBack }: AssistantSetupProps) {
  const { saveConfig, models, setModels, error, setError } = useAssistantStore(
    useShallow((s) => ({
      saveConfig: s.saveConfig,
      models: s.models,
      setModels: s.setModels,
      error: s.error,
      setError: s.setError,
    }))
  );

  const [apiKey, setApiKey] = useState("");
  const [selectedModel, setSelectedModel] = useState("");
  const [loadingModels, setLoadingModels] = useState(false);

  const fetchModels = async () => {
    if (!apiKey.trim()) return;
    setLoadingModels(true);
    setError(null);

    try {
      const response = await fetch("https://openrouter.ai/api/v1/models", {
        headers: { Authorization: `Bearer ${apiKey}` },
      });

      if (!response.ok) {
        setError("Invalid API key");
        setLoadingModels(false);
        return;
      }

      const data = await response.json();
      const modelList = (data.data || [])
        .map((m: any) => ({ id: m.id, name: m.name || m.id }))
        .sort((a: any, b: any) => a.name.localeCompare(b.name));

      setModels(modelList);
      // Default to Sonnet if available
      const defaultModel = modelList.find((m: any) => m.id.includes("claude-sonnet")) || modelList[0];
      if (defaultModel) setSelectedModel(defaultModel.id);
    } catch (e) {
      setError(String(e));
    } finally {
      setLoadingModels(false);
    }
  };

  const handleSave = async () => {
    if (!apiKey.trim() || !selectedModel) return;
    await saveConfig("openrouter", apiKey, selectedModel);
  };

  return (
    <div className="flex flex-1 flex-col items-center justify-center p-4">
      <div className="w-full max-w-xs space-y-3">
        <div className="flex items-center gap-2">
          {onBack && (
            <button
              onClick={onBack}
              className="text-xs text-text-tertiary hover:text-text-secondary"
            >
              ← Back
            </button>
          )}
          <h3 className="text-xs font-semibold uppercase tracking-wider text-zinc-400">
            Assistant Setup
          </h3>
        </div>

        <div>
          <label className="mb-1 block text-[10px] text-zinc-500">Provider</label>
          <select
            className="w-full rounded border border-surface-3 bg-surface-0 px-2 py-1.5 text-xs text-zinc-300 outline-none focus:border-accent"
            value="openrouter"
            disabled
          >
            <option value="openrouter">OpenRouter</option>
          </select>
        </div>

        <div>
          <label className="mb-1 block text-[10px] text-zinc-500">API Key</label>
          <input
            type="password"
            value={apiKey}
            onChange={(e) => setApiKey(e.target.value)}
            onBlur={fetchModels}
            onKeyDown={(e) => e.key === "Enter" && !e.nativeEvent.isComposing && fetchModels()}
            placeholder="sk-or-..."
            className="w-full rounded border border-surface-3 bg-surface-0 px-2 py-1.5 text-xs text-zinc-300 placeholder-zinc-600 outline-none focus:border-accent"
          />
        </div>

        <div>
          <label className="mb-1 block text-[10px] text-zinc-500">Model</label>
          <select
            value={selectedModel}
            onChange={(e) => setSelectedModel(e.target.value)}
            disabled={models.length === 0}
            className="w-full rounded border border-surface-3 bg-surface-0 px-2 py-1.5 text-xs text-zinc-300 outline-none focus:border-accent disabled:opacity-50"
          >
            {models.length === 0 ? (
              <option>{loadingModels ? "Loading models..." : "Enter API key first"}</option>
            ) : (
              models.map((m) => (
                <option key={m.id} value={m.id}>
                  {m.name}
                </option>
              ))
            )}
          </select>
        </div>

        {error && (
          <p className="rounded bg-red-500/10 px-2 py-1 text-[10px] text-red-400">
            {error}
          </p>
        )}

        <button
          onClick={handleSave}
          disabled={!apiKey.trim() || !selectedModel}
          className="w-full rounded bg-accent px-3 py-1.5 text-xs font-medium text-white transition-colors duration-150 hover:bg-accent-hover disabled:opacity-50"
        >
          Save
        </button>
      </div>
    </div>
  );
}
