export interface SessionCost {
  session_id: string;
  input_tokens: number;
  output_tokens: number;
  cache_creation_tokens: number;
  cache_read_tokens: number;
  modified_at: number;
}

export interface ProjectCosts {
  sessions: SessionCost[];
  total_input_tokens: number;
  total_output_tokens: number;
  total_cache_creation_tokens: number;
  total_cache_read_tokens: number;
  week_input_tokens: number;
  week_output_tokens: number;
}
