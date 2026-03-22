export type TaskStatus = "open" | "working" | "closed";

export interface Task {
  id: number;
  repo_id: number;
  description: string;
  images: string[];
  status: TaskStatus;
  session_id: number | null;
  created_at: string;
  updated_at: string;
  supervisor_status?: string | null;
  retry_count: number;
  last_retry_at?: string | null;
  max_retries: number;
}

export interface DraftImage {
  filename: string;
  objectUrl: string;
}
