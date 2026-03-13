import type { AgentState, AgentLoopConfig, AgentMessage } from "@mariozechner/pi-agent-core";
import type { Message } from "@mariozechner/pi-ai";
import { tools } from "./tools.js";
import type { HistoryMessage } from "./protocol.js";

const SYSTEM_PROMPT = `You are the Racc assistant — a global operations butler for a developer running multiple AI coding agents in parallel.

Today's date: ${new Date().toISOString().split("T")[0]}

Your primary job: help the developer understand and review what their agents have done, without requiring them to read every line of every diff.

When summarizing changes:
- Lead with a high-level summary (what changed, why it likely changed)
- Categorize files by review priority:
  HIGH: security-sensitive, architectural, config, database
  MEDIUM: business logic, API changes
  LOW: tests, types, formatting, generated files
- Flag specific concerns (unparameterized SQL, hardcoded secrets, missing error handling, breaking API changes)
- Be concise — the developer has multiple agents to review

You have access to all sessions, their diffs, and their costs. Answer questions about any session's work.`;

export function createAgentState(): AgentState {
  return {
    systemPrompt: SYSTEM_PROMPT,
    model: null as any, // Set when config is received
    thinkingLevel: "off",
    tools,
    messages: [],
    isStreaming: false,
    streamMessage: null,
    pendingToolCalls: new Set(),
  };
}

export function hydrateHistory(state: AgentState, messages: HistoryMessage[]): void {
  for (const msg of messages) {
    if (msg.role === "user") {
      state.messages.push({
        role: "user",
        content: msg.content,
        timestamp: Date.now(),
      });
    } else if (msg.role === "assistant") {
      state.messages.push({
        role: "assistant",
        content: [{ type: "text", text: msg.content }],
        api: "openai-completions",
        provider: "openrouter",
        model: "",
        usage: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0, totalTokens: 0, cost: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0, total: 0 } },
        stopReason: "stop",
        timestamp: Date.now(),
      });
    }
    // tool_call and tool_result are omitted in v1 hydration for simplicity
  }
}

export function createLoopConfig(state: AgentState, apiKey: string): AgentLoopConfig {
  return {
    model: state.model,
    apiKey,
    convertToLlm: (messages: AgentMessage[]): Message[] => {
      return messages.filter(
        (m): m is Message => "role" in m && (m.role === "user" || m.role === "assistant" || m.role === "toolResult"),
      );
    },
  };
}
