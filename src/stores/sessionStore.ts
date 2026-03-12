import { create } from "zustand";
import { invoke } from "@tauri-apps/api/core";
import type { Session } from "../types/session";

interface SessionState {
  sessions: Session[];
  activeSessionId: string | null;
  loading: boolean;
  error: string | null;

  setActiveSession: (id: string) => void;
  fetchSessions: () => Promise<void>;
  createSession: (
    repoPath: string,
    project: string,
    branch: string,
    agent: string,
  ) => Promise<void>;
  stopSession: (id: string) => Promise<void>;
  clearError: () => void;
}

export const useSessionStore = create<SessionState>((set, get) => ({
  sessions: [],
  activeSessionId: null,
  loading: false,
  error: null,

  setActiveSession: (id) => set({ activeSessionId: id }),

  clearError: () => set({ error: null }),

  fetchSessions: async () => {
    try {
      const sessions = await invoke<Session[]>("list_sessions");
      const state = get();
      const activeStillExists = sessions.some(
        (s) => s.id === state.activeSessionId,
      );
      set({
        sessions,
        loading: false,
        activeSessionId: activeStillExists ? state.activeSessionId : null,
      });
    } catch {
      // tmux might not be running — that's OK, just show empty
      set({ sessions: [], loading: false });
    }
  },

  createSession: async (repoPath, project, branch, agent) => {
    set({ error: null });
    const session = await invoke<Session>("create_session", {
      repoPath,
      project,
      branch,
      agent,
    });
    set((state) => ({
      sessions: [...state.sessions, session],
      activeSessionId: session.id,
    }));
  },

  stopSession: async (id) => {
    try {
      await invoke("stop_session", { sessionId: id });
    } catch {
      // Session might already be dead — that's fine
    }
    set((state) => ({
      sessions: state.sessions.filter((s) => s.id !== id),
      activeSessionId:
        state.activeSessionId === id ? null : state.activeSessionId,
    }));
  },
}));
