export type SessionStatus =
  | "Creating"
  | "Running"
  | "Waiting"
  | "Paused"
  | "Disconnected"
  | "Completed"
  | "Error";

export interface Session {
  id: string;
  name: string;
  project: string;
  branch: string;
  agent: string;
  status: SessionStatus;
  worktree_path: string;
}
