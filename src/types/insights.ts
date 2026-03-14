export type InsightType =
  | "repeated_prompt"
  | "startup_pattern"
  | "repeated_permission"
  | "cost_anomaly"
  | "file_conflict"
  | "similar_sessions";

export type InsightSeverity = "warning" | "alert" | "suggestion";

export type InsightStatus = "active" | "applied" | "dismissed" | "expired";

export type SessionEventType =
  | "user_input"
  | "permission_request"
  | "file_operation"
  | "cost_update"
  | "session_meta";

export interface SessionEvent {
  sessionId: number;
  eventType: SessionEventType;
  payload: Record<string, unknown>;
  createdAt: number;
}

export interface Insight {
  id: number;
  insight_type: InsightType;
  severity: InsightSeverity;
  title: string;
  summary: string;
  detail_json: string;
  fingerprint: string;
  status: InsightStatus;
  created_at: number;
  resolved_at: number | null;
}

export interface RepeatedPromptDetail {
  matches: Array<{
    sessionId: number;
    branch: string | null;
    text: string;
    timestamp: number;
  }>;
  suggestedEntry?: string;
}

export interface FileConflictDetail {
  filePath: string;
  sessions: Array<{
    sessionId: number;
    branch: string | null;
    operation: string;
    timestamp: number;
  }>;
}

export interface CostAnomalyDetail {
  sessionId: number;
  currentCost: number;
  averageCost: number;
  windowMinutes: number;
}

export interface RepeatedPermissionDetail {
  sessionId: number;
  permissionType: string;
  count: number;
}

export interface StartupPatternDetail {
  commands: string[];
  sessions: Array<{ sessionId: number; branch: string | null }>;
  suggestedEntry?: string;
}

export interface SimilarSessionsDetail {
  sessionA: { id: number; branch: string | null };
  sessionB: { id: number; branch: string | null };
  similarity: number;
  sharedFiles: string[];
}
