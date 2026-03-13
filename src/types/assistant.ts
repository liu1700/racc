export interface AssistantMessage {
  id: number;
  role: "user" | "assistant" | "tool_call" | "tool_result";
  content: string;
  tool_name?: string;
  tool_call_id?: string;
  created_at: string;
}

export interface AssistantConfig {
  provider: string | null;
  api_key: string | null;
  model: string | null;
}

export interface ModelOption {
  id: string;
  name: string;
}
