export type MergeItemStatus =
  | "queued"
  | "shipping"
  | "succeeded"
  | "failed"
  | "needs_review";

export type MergeRunStatus =
  | "starting"
  | "shipping"
  | "succeeded"
  | "failed"
  | "needs_review";

export interface MergeSettings {
  repo_id: number;
  target_branch: string;
  agent: "claude-code" | "codex";
  instructions: string;
  updated_at: string | null;
}

export interface MergeQueueItem {
  id: number;
  repo_id: number;
  task_id: number;
  source_session_id: number;
  pr_url: string;
  status: MergeItemStatus;
  run_id: number | null;
  result_message: string | null;
  added_at: string;
  updated_at: string;
}

export interface MergeRun {
  id: number;
  repo_id: number;
  session_id: number | null;
  target_branch: string;
  agent: "claude-code" | "codex";
  integration_branch: string | null;
  prompt: string;
  status: MergeRunStatus;
  result_json: string | null;
  created_at: string;
  updated_at: string;
}

export interface MergeManagerState {
  settings: MergeSettings;
  items: MergeQueueItem[];
  active_run: MergeRun | null;
  last_run: MergeRun | null;
}

export interface ShipResult {
  run_id: number;
  status: "succeeded" | "failed";
  merged_prs: string[];
  failed_prs: Array<{ url: string; reason: string }>;
  tests: Array<{
    command: string;
    status: "passed" | "failed";
    summary?: string;
  }>;
  summary: string;
}
