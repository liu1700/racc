import { create } from "zustand";
import { transport } from "../services/transport";
import type {
  MergeManagerState,
  MergeQueueItem,
  MergeRun,
  MergeSettings,
} from "../types/merge";
import { useSessionStore } from "./sessionStore";

interface MergeState {
  repoId: number | null;
  settings: MergeSettings | null;
  items: MergeQueueItem[];
  activeRun: MergeRun | null;
  lastRun: MergeRun | null;
  loading: boolean;
  saving: boolean;
  shipping: boolean;
  resetting: boolean;
  error: string | null;
  eventsInitialized: boolean;

  initializeEvents: () => void;
  load: (repoId: number) => Promise<void>;
  setReady: (taskId: number, ready: boolean) => Promise<void>;
  saveSettings: (settings: Pick<MergeSettings, "target_branch" | "agent" | "instructions">) => Promise<void>;
  startRun: () => Promise<MergeRun>;
  resolveRun: (status: "succeeded" | "failed") => Promise<void>;
  retryRun: () => Promise<MergeRun>;
  resetManager: () => Promise<void>;
  clearError: () => void;
}

async function fetchManager(repoId: number): Promise<MergeManagerState> {
  return await transport.call("get_merge_manager", { repoId }) as MergeManagerState;
}

export const useMergeStore = create<MergeState>((set, get) => ({
  repoId: null,
  settings: null,
  items: [],
  activeRun: null,
  lastRun: null,
  loading: false,
  saving: false,
  shipping: false,
  resetting: false,
  error: null,
  eventsInitialized: false,

  initializeEvents: () => {
    if (get().eventsInitialized) return;
    set({ eventsInitialized: true });

    const reloadIfCurrent = (data: { repo_id?: number }) => {
      const repoId = get().repoId;
      if (repoId != null && data?.repo_id === repoId) {
        void get().load(repoId);
      }
    };
    transport.on("merge_manager_changed", reloadIfCurrent);
    transport.on("racc://event", (event: { event?: string; data?: { repo_id?: number } }) => {
      if (event?.event === "merge_manager_changed" && event.data) {
        reloadIfCurrent(event.data);
      }
    });
  },

  load: async (repoId) => {
    set({ repoId, loading: true, error: null });
    try {
      const state = await fetchManager(repoId);
      if (get().repoId !== repoId) return;
      set({
        settings: state.settings,
        items: state.items,
        activeRun: state.active_run,
        lastRun: state.last_run,
        loading: false,
      });
    } catch (error) {
      if (get().repoId === repoId) {
        set({ loading: false, error: String(error) });
      }
    }
  },

  setReady: async (taskId, ready) => {
    const repoId = get().repoId;
    if (repoId == null) throw new Error("No repository selected");
    set({ error: null });
    try {
      await transport.call("set_task_ready_to_merge", { taskId, ready });
      const state = await fetchManager(repoId);
      set({
        settings: state.settings,
        items: state.items,
        activeRun: state.active_run,
        lastRun: state.last_run,
      });
    } catch (error) {
      set({ error: String(error) });
      throw error;
    }
  },

  saveSettings: async (settings) => {
    const repoId = get().repoId;
    if (repoId == null) return;
    set({ saving: true, error: null });
    try {
      const saved = await transport.call("update_merge_settings", {
        repoId,
        targetBranch: settings.target_branch,
        agent: settings.agent,
        instructions: settings.instructions,
      }) as MergeSettings;
      set({ settings: saved, saving: false });
    } catch (error) {
      set({ saving: false, error: String(error) });
      throw error;
    }
  },

  startRun: async () => {
    const repoId = get().repoId;
    if (repoId == null) throw new Error("No repository selected");
    set({ shipping: true, error: null });
    try {
      const run = await transport.call("start_merge_run", { repoId }) as MergeRun;
      await useSessionStore.getState().refreshRepos(run.session_id ?? undefined);
      const state = await fetchManager(repoId);
      set({
        settings: state.settings,
        items: state.items,
        activeRun: state.active_run,
        lastRun: state.last_run,
        shipping: false,
      });
      return run;
    } catch (error) {
      set({ shipping: false, error: String(error) });
      throw error;
    }
  },

  resolveRun: async (status) => {
    const run = get().lastRun;
    const repoId = get().repoId;
    if (!run || repoId == null) return;
    set({ error: null });
    try {
      await transport.call("resolve_merge_run", { runId: run.id, status });
      await get().load(repoId);
    } catch (error) {
      set({ error: String(error) });
      throw error;
    }
  },

  retryRun: async () => {
    const run = get().lastRun;
    const repoId = get().repoId;
    if (!run || repoId == null) throw new Error("No merge run to retry");
    set({ shipping: true, error: null });
    try {
      const next = await transport.call("retry_merge_run", { runId: run.id }) as MergeRun;
      await useSessionStore.getState().refreshRepos(next.session_id ?? undefined);
      await get().load(repoId);
      set({ shipping: false });
      return next;
    } catch (error) {
      set({ shipping: false, error: String(error) });
      throw error;
    }
  },

  resetManager: async () => {
    const repoId = get().repoId;
    if (repoId == null) throw new Error("No repository selected");
    set({ resetting: true, error: null });
    try {
      await transport.call("reset_merge_manager", { repoId });
      const state = await fetchManager(repoId);
      if (get().repoId === repoId) {
        set({
          settings: state.settings,
          items: state.items,
          activeRun: state.active_run,
          lastRun: state.last_run,
          resetting: false,
        });
      } else {
        set({ resetting: false });
      }
    } catch (error) {
      set({ resetting: false, error: String(error) });
      throw error;
    }
  },

  clearError: () => set({ error: null }),
}));
