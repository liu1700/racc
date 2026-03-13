import { create } from "zustand";
import { invoke } from "@tauri-apps/api/core";
import type { AssistantMessage, AssistantConfig, ModelOption } from "../types/assistant";

interface AssistantState {
  messages: AssistantMessage[];
  isStreaming: boolean;
  streamingText: string;
  config: AssistantConfig | null;
  models: ModelOption[];
  assistantCost: number;
  error: string | null;

  loadConfig: () => Promise<void>;
  saveConfig: (provider: string, apiKey: string, model: string) => Promise<void>;
  loadHistory: () => Promise<void>;
  sendMessage: (content: string) => Promise<void>;
  appendChunk: (text: string) => void;
  finishStreaming: (usage: { input_tokens: number; output_tokens: number; cost_usd: number }) => void;
  setModels: (models: ModelOption[]) => void;
  setError: (error: string | null) => void;
  clearError: () => void;
}

export const useAssistantStore = create<AssistantState>((set, get) => ({
  messages: [],
  isStreaming: false,
  streamingText: "",
  config: null,
  models: [],
  assistantCost: 0,
  error: null,

  loadConfig: async () => {
    try {
      const config = await invoke<AssistantConfig>("get_assistant_config");
      set({ config });
    } catch (e) {
      set({ error: String(e) });
    }
  },

  saveConfig: async (provider, apiKey, model) => {
    try {
      await invoke("set_assistant_config", { provider, apiKey, model });
      set({ config: { provider, api_key: apiKey, model } });
    } catch (e) {
      set({ error: String(e) });
    }
  },

  loadHistory: async () => {
    try {
      const messages = await invoke<AssistantMessage[]>("get_assistant_messages", { limit: 50 });
      set({ messages });
    } catch (e) {
      set({ error: String(e) });
    }
  },

  sendMessage: async (content) => {
    const userMsg: AssistantMessage = {
      id: Date.now(),
      role: "user",
      content,
      created_at: new Date().toISOString(),
    };

    set((s) => ({
      messages: [...s.messages, userMsg],
      isStreaming: true,
      streamingText: "",
      error: null,
    }));

    try {
      // Persist user message
      await invoke("save_assistant_message", {
        role: "user",
        content,
        toolName: null,
        toolCallId: null,
      });

      // Send to sidecar via Rust backend
      await invoke("assistant_send_message", { content });
    } catch (e) {
      set({ isStreaming: false, error: String(e) });
    }
  },

  appendChunk: (text) => {
    set((s) => ({ streamingText: s.streamingText + text }));
  },

  finishStreaming: (usage) => {
    const { streamingText } = get();
    if (streamingText) {
      const assistantMsg: AssistantMessage = {
        id: Date.now(),
        role: "assistant",
        content: streamingText,
        created_at: new Date().toISOString(),
      };
      set((s) => ({
        messages: [...s.messages, assistantMsg],
        isStreaming: false,
        streamingText: "",
        assistantCost: s.assistantCost + usage.cost_usd,
      }));

      // Persist assistant message (fire-and-forget)
      invoke("save_assistant_message", {
        role: "assistant",
        content: streamingText,
        toolName: null,
        toolCallId: null,
      }).catch(() => {});
    } else {
      set({ isStreaming: false, streamingText: "" });
    }
  },

  setModels: (models) => set({ models }),
  setError: (error) => set({ error }),
  clearError: () => set({ error: null }),
}));
