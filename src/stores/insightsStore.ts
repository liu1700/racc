import { create } from "zustand";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { addEventListener } from "../services/eventCapture";
import type {
  Insight,
  SessionEvent,
  FileConflictDetail,
  CostAnomalyDetail,
  RepeatedPermissionDetail,
} from "../types/insights";

interface InsightsState {
  insights: Insight[];
  expandedId: number | null;
  loading: boolean;

  // Real-time rule state — intentionally mutable Maps, not reactive.
  // These are internal tracking state, not rendered directly.
  _fileMap: Map<string, Set<number>>; // filePath → sessionIds
  _permissionCounts: Map<number, Map<string, number>>; // sessionId → permType → count
  _costHistory: Map<number, number[]>; // sessionId → rolling cost deltas
  _initialized: boolean;

  // Actions
  initialize: () => Promise<void>;
  loadInsights: () => Promise<void>;
  toggleExpand: (id: number) => void;
  dismissInsight: (id: number) => Promise<void>;
  applyInsight: (id: number) => Promise<void>;

  // Internal
  _addInsight: (insight: Insight) => void;
  _handleEvent: (event: SessionEvent) => void;
}

export const useInsightsStore = create<InsightsState>((set, get) => ({
  insights: [],
  expandedId: null,
  loading: false,

  _fileMap: new Map(),
  _permissionCounts: new Map(),
  _costHistory: new Map(),
  _initialized: false,

  initialize: async () => {
    // Guard against duplicate initialization (e.g. component remount)
    if (get()._initialized) return;
    set({ _initialized: true });

    // Load existing insights from SQLite
    await get().loadInsights();

    // Subscribe to real-time events from eventCapture
    addEventListener((event) => get()._handleEvent(event));

    // Listen for batch analysis results from Rust
    listen<Insight>("insight-detected", (e) => {
      get()._addInsight(e.payload);
    });

    // Set up batch analysis polling (every 5 minutes)
    // Silently catches errors — run_batch_analysis may not exist until batch analysis is deployed
    setInterval(() => {
      invoke("run_batch_analysis").catch(() => {});
    }, 5 * 60 * 1000);
  },

  loadInsights: async () => {
    set({ loading: true });
    try {
      const insights = await invoke<Insight[]>("get_insights", { status: "active" });
      set({ insights, loading: false });
    } catch (e) {
      console.error("[insights] load failed:", e);
      set({ loading: false });
    }
  },

  toggleExpand: (id) => {
    set((s) => ({ expandedId: s.expandedId === id ? null : id }));
  },

  dismissInsight: async (id) => {
    try {
      await invoke("update_insight_status", { id, status: "dismissed" });
      set((s) => ({
        insights: s.insights.filter((i) => i.id !== id),
        expandedId: s.expandedId === id ? null : s.expandedId,
      }));
    } catch (e) {
      console.error("[insights] dismiss failed:", e);
    }
  },

  applyInsight: async (id) => {
    try {
      await invoke("update_insight_status", { id, status: "applied" });
      set((s) => ({
        insights: s.insights.filter((i) => i.id !== id),
        expandedId: s.expandedId === id ? null : s.expandedId,
      }));
    } catch (e) {
      console.error("[insights] apply failed:", e);
    }
  },

  _addInsight: (insight) => {
    set((s) => {
      if (s.insights.some((i) => i.fingerprint === insight.fingerprint)) return s;
      return { insights: [insight, ...s.insights] };
    });
  },

  _handleEvent: (event) => {
    const state = get();

    switch (event.eventType) {
      case "file_operation": {
        const { operation, filePath } = event.payload as { operation: string; filePath: string };
        if (operation !== "edit" && operation !== "write") break;

        const fileMap = state._fileMap;
        if (!fileMap.has(filePath)) fileMap.set(filePath, new Set());
        fileMap.get(filePath)!.add(event.sessionId);

        if (fileMap.get(filePath)!.size > 1) {
          const sessions = Array.from(fileMap.get(filePath)!);
          const fingerprint = `file_conflict:${filePath}:${sessions.sort().join(",")}`;

          if (state.insights.some((i) => i.fingerprint === fingerprint)) break;

          const detail: FileConflictDetail = {
            filePath,
            sessions: sessions.map((sid) => ({
              sessionId: sid,
              branch: null,
              operation,
              timestamp: event.createdAt,
            })),
          };

          invoke<number | null>("save_insight", {
            insightType: "file_conflict",
            severity: "alert",
            title: `File conflict: ${filePath.split("/").pop()}`,
            summary: `Modified in ${sessions.length} sessions`,
            detailJson: JSON.stringify(detail),
            fingerprint,
          }).then((id) => {
            if (id != null) {
              get()._addInsight({
                id,
                insight_type: "file_conflict",
                severity: "alert",
                title: `File conflict: ${filePath.split("/").pop()}`,
                summary: `Modified in ${sessions.length} sessions`,
                detail_json: JSON.stringify(detail),
                fingerprint,
                status: "active",
                created_at: Date.now(),
                resolved_at: null,
              });
            }
          });
        }
        break;
      }

      case "cost_update": {
        const { estimatedCostUsd } = event.payload as { estimatedCostUsd: number };
        const history = state._costHistory;
        if (!history.has(event.sessionId)) history.set(event.sessionId, []);
        const costs = history.get(event.sessionId)!;
        costs.push(estimatedCostUsd);

        if (costs.length > 10) costs.shift();

        if (costs.length >= 3) {
          const avg = costs.slice(0, -1).reduce((a, b) => a + b, 0) / (costs.length - 1);
          const current = costs[costs.length - 1];

          if (current > avg * 3 && current > 0.5) {
            const fingerprint = `cost_anomaly:${event.sessionId}:${Math.floor(Date.now() / 600_000)}`;
            if (state.insights.some((i) => i.fingerprint === fingerprint)) break;

            const detail: CostAnomalyDetail = {
              sessionId: event.sessionId,
              currentCost: current,
              averageCost: avg,
              windowMinutes: 10,
            };

            invoke<number | null>("save_insight", {
              insightType: "cost_anomaly",
              severity: "alert",
              title: `Cost spike: session ${event.sessionId}`,
              summary: `$${current.toFixed(2)} in last interval (avg $${avg.toFixed(2)})`,
              detailJson: JSON.stringify(detail),
              fingerprint,
            }).then((id) => {
              if (id != null) {
                get()._addInsight({
                  id,
                  insight_type: "cost_anomaly",
                  severity: "alert",
                  title: `Cost spike: session ${event.sessionId}`,
                  summary: `$${current.toFixed(2)} in last interval (avg $${avg.toFixed(2)})`,
                  detail_json: JSON.stringify(detail),
                  fingerprint,
                  status: "active",
                  created_at: Date.now(),
                  resolved_at: null,
                });
              }
            });
          }
        }
        break;
      }

      case "permission_request": {
        const { permissionType } = event.payload as { permissionType: string };
        const permMap = state._permissionCounts;
        if (!permMap.has(event.sessionId)) permMap.set(event.sessionId, new Map());
        const sessionPerms = permMap.get(event.sessionId)!;
        const count = (sessionPerms.get(permissionType) || 0) + 1;
        sessionPerms.set(permissionType, count);

        if (count === 3) {
          const fingerprint = `repeated_perm:${event.sessionId}:${permissionType}`;
          if (state.insights.some((i) => i.fingerprint === fingerprint)) break;

          const detail: RepeatedPermissionDetail = {
            sessionId: event.sessionId,
            permissionType,
            count,
          };

          invoke<number | null>("save_insight", {
            insightType: "repeated_permission",
            severity: "warning",
            title: "Repeated permission requests",
            summary: `"${permissionType}" requested ${count} times`,
            detailJson: JSON.stringify(detail),
            fingerprint,
          }).then((id) => {
            if (id != null) {
              get()._addInsight({
                id,
                insight_type: "repeated_permission",
                severity: "warning",
                title: "Repeated permission requests",
                summary: `"${permissionType}" requested ${count} times`,
                detail_json: JSON.stringify(detail),
                fingerprint,
                status: "active",
                created_at: Date.now(),
                resolved_at: null,
              });
            }
          });
        }
        break;
      }
    }
  },
}));
