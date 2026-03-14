export interface Repo {
  id: number;
  path: string;
  name: string;
  added_at: string;
}

export type SessionStatus = "Running" | "Completed" | "Disconnected" | "Error";

export interface Session {
  id: number;
  repo_id: number;
  agent: string;
  worktree_path: string | null;
  branch: string | null;
  status: SessionStatus;
  created_at: string;
  updated_at: string;
  pr_url: string | null;
}

export interface RepoWithSessions {
  repo: Repo;
  sessions: Session[];
}
