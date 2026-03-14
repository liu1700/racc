export type TaskStatus = "open" | "working" | "closed";

export interface Task {
  id: number;
  repo_id: number;
  description: string;
  status: TaskStatus;
  session_id: number | null;
  created_at: string;
  updated_at: string;
}
