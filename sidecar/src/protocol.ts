// --- Inbound messages (Rust → Sidecar) ---

export type InboundMessage =
  | { type: "user_message"; content: string }
  | { type: "tool_result"; call_id: string; content: string }
  | { type: "set_config"; provider: string; api_key: string; model: string }
  | { type: "history"; messages: HistoryMessage[] }
  | { type: "shutdown" };

export interface HistoryMessage {
  role: "user" | "assistant" | "tool_call" | "tool_result";
  content: string;
  tool_name?: string;
  tool_call_id?: string;
}

// --- Outbound messages (Sidecar → Rust) ---

export type OutboundMessage =
  | { type: "chunk"; text: string }
  | { type: "tool_call"; id: string; name: string; args: Record<string, unknown> }
  | { type: "done"; usage: { input_tokens: number; output_tokens: number; cost_usd: number } }
  | { type: "error"; message: string }
  | { type: "models"; models: { id: string; name: string }[] };

export function sendMessage(msg: OutboundMessage): void {
  process.stdout.write(JSON.stringify(msg) + "\n");
}

export function parseInbound(line: string): InboundMessage | null {
  try {
    return JSON.parse(line) as InboundMessage;
  } catch {
    return null;
  }
}
