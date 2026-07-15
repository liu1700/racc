export type TestRunStatus =
  | "starting"
  | "testing"
  | "succeeded"
  | "failed"
  | "needs_review";

export interface TestSettings {
  repo_id: number;
  target_branch: string;
  agent: "claude-code" | "codex";
  instructions: string;
  updated_at: string | null;
}

export interface TestRun {
  id: number;
  repo_id: number;
  session_id: number | null;
  target_branch: string;
  agent: "claude-code" | "codex";
  worktree_branch: string | null;
  prompt: string;
  status: TestRunStatus;
  result_json: string | null;
  created_at: string;
  updated_at: string;
}

export interface TestManagerState {
  settings: TestSettings;
  active_run: TestRun | null;
  last_run: TestRun | null;
}

export interface TestResult {
  run_id: number;
  status: "succeeded" | "failed";
  tests: Array<{
    name: string;
    status: "passed" | "failed";
    summary?: string;
  }>;
  summary: string;
}
