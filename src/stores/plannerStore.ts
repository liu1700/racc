import { create } from "zustand";
import { transport } from "../services/transport";
import type { TaskPlanRun } from "../types/planner";
import type { Task } from "../types/task";
import { useSessionStore } from "./sessionStore";
import { useTaskStore } from "./taskStore";

interface PlannerState {
  repoId: number | null;
  run: TaskPlanRun | null;
  loading: boolean;
  starting: boolean;
  confirming: boolean;
  error: string | null;
  eventsInitialized: boolean;

  initializeEvents: () => void;
  load: (repoId: number) => Promise<void>;
  start: (
    repoId: number,
    sourceInput: string,
    agent: "claude-code" | "codex",
  ) => Promise<TaskPlanRun>;
  confirm: (selectedKeys: string[]) => Promise<Task[]>;
  clearError: () => void;
}

async function fetchLatest(repoId: number): Promise<TaskPlanRun | null> {
  return await transport.call("get_latest_task_plan", { repoId }) as TaskPlanRun | null;
}

export const usePlannerStore = create<PlannerState>((set, get) => ({
  repoId: null,
  run: null,
  loading: false,
  starting: false,
  confirming: false,
  error: null,
  eventsInitialized: false,

  initializeEvents: () => {
    if (get().eventsInitialized) return;
    set({ eventsInitialized: true });

    const reloadIfCurrent = (data: { repo_id?: number; run_id?: number }) => {
      const repoId = get().repoId;
      if (repoId != null && data?.repo_id === repoId) {
        void get().load(repoId).then(() => {
          const run = get().run;
          if (run && run.id === data.run_id && run.status === "completed") {
            void useTaskStore.getState().loadTasks(repoId);
          }
        });
      }
    };
    transport.on("task_plan_changed", reloadIfCurrent);
    transport.on(
      "racc://event",
      (event: { event?: string; data?: { repo_id?: number; run_id?: number } }) => {
        if (event?.event === "task_plan_changed" && event.data) {
          reloadIfCurrent(event.data);
        }
      },
    );
  },

  load: async (repoId) => {
    set({ repoId, loading: true });
    try {
      const run = await fetchLatest(repoId);
      if (get().repoId !== repoId) return;
      set({ run, loading: false, error: null });
    } catch (error) {
      if (get().repoId === repoId) {
        set({ loading: false, error: String(error) });
      }
    }
  },

  start: async (repoId, sourceInput, agent) => {
    set({ repoId, starting: true, error: null });
    try {
      const run = await transport.call("start_task_plan", {
        repoId,
        sourceInput,
        agent,
      }) as TaskPlanRun;
      set({ run, starting: false });
      if (run.session_id != null) {
        await useSessionStore.getState().refreshRepos(run.session_id);
      }
      return run;
    } catch (error) {
      set({ starting: false, error: String(error) });
      const latest = await fetchLatest(repoId).catch(() => null);
      if (latest) set({ run: latest });
      throw error;
    }
  },

  confirm: async (selectedKeys) => {
    const run = get().run;
    if (!run) throw new Error("No task plan to confirm");
    set({ confirming: true, error: null });
    try {
      const tasks = await transport.call("confirm_task_plan", {
        runId: run.id,
        selectedKeys,
      }) as Task[];
      await useTaskStore.getState().loadTasks(run.repo_id);
      const latest = await fetchLatest(run.repo_id);
      set({ run: latest, confirming: false });
      return tasks;
    } catch (error) {
      set({ confirming: false, error: String(error) });
      throw error;
    }
  },

  clearError: () => set({ error: null }),
}));
