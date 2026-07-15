import { create } from "zustand";
import { transport } from "../services/transport";
import type {
  TestManagerState,
  TestRun,
  TestSettings,
} from "../types/testManager";
import { useSessionStore } from "./sessionStore";

interface TestManagerStoreState {
  repoId: number | null;
  settings: TestSettings | null;
  activeRun: TestRun | null;
  lastRun: TestRun | null;
  loading: boolean;
  saving: boolean;
  starting: boolean;
  resetting: boolean;
  error: string | null;
  eventsInitialized: boolean;

  initializeEvents: () => void;
  load: (repoId: number) => Promise<void>;
  saveSettings: (
    settings: Pick<TestSettings, "target_branch" | "agent" | "instructions">,
  ) => Promise<void>;
  startRun: () => Promise<TestRun>;
  resolveRun: (status: "succeeded" | "failed") => Promise<void>;
  retryRun: () => Promise<TestRun>;
  resetManager: () => Promise<void>;
  clearError: () => void;
}

async function fetchManager(repoId: number): Promise<TestManagerState> {
  return await transport.call("get_test_manager", { repoId }) as TestManagerState;
}

export const useTestManagerStore = create<TestManagerStoreState>((set, get) => ({
  repoId: null,
  settings: null,
  activeRun: null,
  lastRun: null,
  loading: false,
  saving: false,
  starting: false,
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
    transport.on("test_manager_changed", reloadIfCurrent);
    transport.on("racc://event", (event: { event?: string; data?: { repo_id?: number } }) => {
      if (event?.event === "test_manager_changed" && event.data) {
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

  saveSettings: async (settings) => {
    const repoId = get().repoId;
    if (repoId == null) return;
    set({ saving: true, error: null });
    try {
      const saved = await transport.call("update_test_settings", {
        repoId,
        targetBranch: settings.target_branch,
        agent: settings.agent,
        instructions: settings.instructions,
      }) as TestSettings;
      set({ settings: saved, saving: false });
    } catch (error) {
      set({ saving: false, error: String(error) });
      throw error;
    }
  },

  startRun: async () => {
    const repoId = get().repoId;
    if (repoId == null) throw new Error("No repository selected");
    set({ starting: true, error: null });
    try {
      const run = await transport.call("start_test_run", { repoId }) as TestRun;
      await useSessionStore.getState().refreshRepos(run.session_id ?? undefined);
      const state = await fetchManager(repoId);
      set({
        settings: state.settings,
        activeRun: state.active_run,
        lastRun: state.last_run,
        starting: false,
      });
      return run;
    } catch (error) {
      set({ starting: false, error: String(error) });
      throw error;
    }
  },

  resolveRun: async (status) => {
    const run = get().lastRun;
    const repoId = get().repoId;
    if (!run || repoId == null) return;
    set({ error: null });
    try {
      await transport.call("resolve_test_run", { runId: run.id, status });
      await get().load(repoId);
    } catch (error) {
      set({ error: String(error) });
      throw error;
    }
  },

  retryRun: async () => {
    const run = get().lastRun;
    const repoId = get().repoId;
    if (!run || repoId == null) throw new Error("No test run to retry");
    set({ starting: true, error: null });
    try {
      const next = await transport.call("retry_test_run", { runId: run.id }) as TestRun;
      await useSessionStore.getState().refreshRepos(next.session_id ?? undefined);
      await get().load(repoId);
      set({ starting: false });
      return next;
    } catch (error) {
      set({ starting: false, error: String(error) });
      throw error;
    }
  },

  resetManager: async () => {
    const repoId = get().repoId;
    if (repoId == null) throw new Error("No repository selected");
    set({ resetting: true, error: null });
    try {
      await transport.call("reset_test_manager", { repoId });
      const state = await fetchManager(repoId);
      if (get().repoId === repoId) {
        set({
          settings: state.settings,
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
