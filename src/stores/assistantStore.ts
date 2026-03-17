import { create } from "zustand";
import { transport } from "../services/transport";
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
      const config = await transport.call("get_assistant_config") as AssistantConfig;
      set({ config });
    } catch (e) {
      set({ error: String(e) });
    }
  },

  saveConfig: async (provider, apiKey, model) => {
    try {
      await transport.call("set_assistant_config", { provider, apiKey, model });
      set({ config: { provider, api_key: apiKey, model } });
    } catch (e) {
      set({ error: String(e) });
    }
  },

  loadHistory: async () => {
    try {
      const messages = await transport.call("get_assistant_messages", { limit: 50 }) as AssistantMessage[];
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
      await transport.call("save_assistant_message", {
        role: "user",
        content,
        toolName: null,
        toolCallId: null,
      });

      await transport.call("assistant_send_message", { content });

      // Poll for responses
      let done = false;
      while (!done) {
        try {
          // assistant_read_response uses async I/O — it awaits the next
          // sidecar output line and handles tool calls internally (looping
          // until a non-tool-call message is ready), so this is not a busy poll.
          const line = await transport.call("assistant_read_response") as string;
          if (!line) {
            // Empty response — add a small delay before retrying
            await new Promise((r) => setTimeout(r, 50));
            continue;
          }

          const msg = JSON.parse(line);
          switch (msg.type) {
            case "chunk":
              get().appendChunk(msg.text);
              break;
            case "done":
              get().finishStreaming(msg.usage || { input_tokens: 0, output_tokens: 0, cost_usd: 0 });
              done = true;
              break;
            case "error":
              set({ isStreaming: false, error: msg.message });
              done = true;
              break;
            case "models":
              set({ models: msg.models });
              break;
            default:
              // Unknown message type (e.g. tool_call that wasn't handled server-side) — skip
              break;
          }
        } catch {
          set({ isStreaming: false, error: "Lost connection to assistant" });
          done = true;
        }
      }
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
      transport.call("save_assistant_message", {
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
