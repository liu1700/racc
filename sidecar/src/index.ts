import * as readline from "node:readline";
import { parseInbound, sendMessage } from "./protocol.js";
import { createAgentState, hydrateHistory, createLoopConfig } from "./agent.js";
import { resolveToolCall } from "./tools.js";
import { agentLoop } from "@mariozechner/pi-agent-core";
import { findModel } from "@mariozechner/pi-ai";
import type { AgentEvent } from "@mariozechner/pi-agent-core";

const state = createAgentState();
let apiKey: string | null = null;
let currentModel: string | null = null;

const rl = readline.createInterface({ input: process.stdin });

rl.on("line", async (line: string) => {
  const msg = parseInbound(line);
  if (!msg) return;

  switch (msg.type) {
    case "shutdown":
      process.exit(0);
      break;

    case "history":
      hydrateHistory(state, msg.messages);
      break;

    case "set_config": {
      apiKey = msg.api_key;
      currentModel = msg.model;
      try {
        const model = findModel(msg.model);
        if (model) {
          state.model = model;
        }
        // Fetch available models from OpenRouter to validate key
        const response = await fetch("https://openrouter.ai/api/v1/models", {
          headers: { Authorization: `Bearer ${msg.api_key}` },
        });
        if (!response.ok) {
          sendMessage({ type: "error", message: "Invalid API key" });
          return;
        }
        const data = await response.json() as { data: { id: string; name: string }[] };
        sendMessage({
          type: "models",
          models: data.data.map((m: any) => ({ id: m.id, name: m.name || m.id })),
        });
      } catch (e) {
        sendMessage({ type: "error", message: String(e) });
      }
      break;
    }

    case "tool_result":
      resolveToolCall(msg.call_id, msg.content);
      break;

    case "user_message": {
      if (!apiKey || !state.model) {
        sendMessage({ type: "error", message: "Assistant not configured. Set API key and model first." });
        return;
      }

      // Add user message to state
      state.messages.push({
        role: "user",
        content: msg.content,
        timestamp: Date.now(),
      });

      try {
        const config = createLoopConfig(state, apiKey);
        let totalUsage = { input: 0, output: 0, cost: 0 };

        for await (const event of agentLoop(state, config) as AsyncIterable<AgentEvent>) {
          switch (event.type) {
            case "message_update":
              if (event.assistantMessageEvent.type === "text_delta") {
                sendMessage({ type: "chunk", text: event.assistantMessageEvent.delta });
              }
              break;
            case "turn_end":
              if ("usage" in event.message && event.message.role === "assistant") {
                const usage = (event.message as any).usage;
                if (usage) {
                  totalUsage.input += usage.input || 0;
                  totalUsage.output += usage.output || 0;
                  totalUsage.cost += usage.cost?.total || 0;
                }
              }
              break;
          }
        }

        sendMessage({
          type: "done",
          usage: {
            input_tokens: totalUsage.input,
            output_tokens: totalUsage.output,
            cost_usd: Math.round(totalUsage.cost * 10000) / 10000,
          },
        });
      } catch (e) {
        sendMessage({ type: "error", message: String(e) });
      }
      break;
    }
  }
});
