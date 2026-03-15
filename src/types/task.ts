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
}

export interface DraftImage {
  filename: string;
  objectUrl: string;
}
