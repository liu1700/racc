import { create } from "zustand";
import { invoke } from "@tauri-apps/api/core";
import type { Session } from "../types/session";

interface SessionState {
  sessions: Session[];
  activeSessionId: string | null;
  loading: boolean;

  setActiveSession: (id: string) => void;
  fetchSessions: () => Promise<void>;
  createSession: (
    project: string,
    branch: string,
    agent: string,
  ) => Promise<void>;
  stopSession: (id: string) => Promise<void>;
}

export const useSessionStore = create<SessionState>((set) => ({
  sessions: [],
  activeSessionId: null,
  loading: false,

  setActiveSession: (id) => set({ activeSessionId: id }),

  fetchSessions: async () => {
    set({ loading: true });
    try {
      const sessions = await invoke<Session[]>("list_sessions");
      set({ sessions, loading: false });
    } catch {
      set({ loading: false });
    }
  },

  createSession: async (project, branch, agent) => {
    const session = await invoke<Session>("create_session", {
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
    await invoke("stop_session", { sessionId: id });
    set((state) => ({
      sessions: state.sessions.filter((s) => s.id !== id),
      activeSessionId:
        state.activeSessionId === id ? null : state.activeSessionId,
    }));
  },
}));
