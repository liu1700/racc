import { Type } from "@sinclair/typebox";
import type { AgentTool, AgentToolResult } from "@mariozechner/pi-agent-core";
import { sendMessage } from "./protocol.js";

// Pending tool calls waiting for results from Rust
const pendingToolCalls = new Map<string, {
  resolve: (result: string) => void;
}>();

export function resolveToolCall(callId: string, content: string): void {
  const pending = pendingToolCalls.get(callId);
  if (pending) {
    pending.resolve(content);
    pendingToolCalls.delete(callId);
  }
}

function createRelayTool(
  name: string,
  description: string,
  label: string,
  parameters: any,
): AgentTool<any> {
  return {
    name,
    description,
    label,
    parameters,
    execute: async (toolCallId, params): Promise<AgentToolResult<any>> => {
      return new Promise((resolve) => {
        pendingToolCalls.set(toolCallId, {
          resolve: (content: string) => {
            resolve({
              content: [{ type: "text", text: content }],
              details: {},
            });
          },
        });

        sendMessage({
          type: "tool_call",
          id: toolCallId,
          name,
          args: params,
        });
      });
    },
  };
}

export const tools: AgentTool<any>[] = [
  createRelayTool(
    "get_all_sessions",
    "Get a list of all coding agent sessions with their status, branch, repo, and elapsed time. Use this to understand what agents are currently running or have completed.",
    "List all sessions",
    Type.Object({}),
  ),
  createRelayTool(
    "get_session_diff",
    "Get the git diff (changes) for a specific session by its ID. Returns the raw git diff HEAD output showing all file changes.",
    "Get session diff",
    Type.Object({
      session_id: Type.Number({ description: "The session ID to get the diff for" }),
    }),
  ),
  createRelayTool(
    "get_session_costs",
    "Get the token usage and estimated cost for a specific session by its ID. Note: costs are per-project, not per-session, so multiple sessions in the same project may show aggregated costs.",
    "Get session costs",
    Type.Object({
      session_id: Type.Number({ description: "The session ID to get costs for" }),
    }),
  ),
];
