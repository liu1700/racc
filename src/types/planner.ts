export type TaskPlanRunStatus =
  | "starting"
  | "planning"
  | "ready"
  | "completed"
  | "failed";

export interface TaskPlanItem {
  key: string;
  title: string;
  description: string;
  acceptance_criteria: string[];
  depends_on: string[];
}

export interface TaskPlanResult {
  run_id: number;
  summary: string;
  tasks: TaskPlanItem[];
}

export interface TaskPlanRun {
  id: number;
  repo_id: number;
  session_id: number | null;
  agent: "claude-code" | "codex";
  source_input: string;
  prompt: string;
  status: TaskPlanRunStatus;
  result_json: string | null;
  error: string | null;
  created_task_ids: string;
  created_at: string;
  updated_at: string;
}
