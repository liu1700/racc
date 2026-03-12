import { create } from "zustand";
import { invoke } from "@tauri-apps/api/core";
import type { Repo, Session, RepoWithSessions } from "../types/session";

interface SessionState {
  repos: RepoWithSessions[];
  activeSessionId: number | null;
  loading: boolean;
  error: string | null;

  getActiveSession: () => { session: Session; repo: Repo } | null;

  initialize: () => Promise<void>;
  importRepo: (path: string) => Promise<void>;
  removeRepo: (repoId: number) => Promise<void>;
  createSession: (
    repoId: number,
    useWorktree: boolean,
    branch?: string,
  ) => Promise<void>;
  stopSession: (sessionId: number) => Promise<void>;
  removeSession: (sessionId: number) => Promise<void>;
  setActiveSession: (id: number) => void;
  clearError: () => void;
}

export const useSessionStore = create<SessionState>((set, get) => ({
  repos: [],
  activeSessionId: null,
  loading: false,
  error: null,

  getActiveSession: () => {
    const { repos, activeSessionId } = get();
    if (activeSessionId === null) return null;
    for (const rws of repos) {
      const session = rws.sessions.find((s) => s.id === activeSessionId);
      if (session) return { session, repo: rws.repo };
    }
    return null;
  },

  initialize: async () => {
    set({ loading: true, error: null });
    try {
      const repos = await invoke<RepoWithSessions[]>("reconcile_sessions");
      set({ repos, loading: false });
    } catch (e) {
      set({ repos: [], loading: false, error: String(e) });
    }
  },

  importRepo: async (path) => {
    set({ error: null });
    try {
      await invoke<Repo>("import_repo", { path });
      const repos = await invoke<RepoWithSessions[]>("list_repos");
      set({ repos });
    } catch (e) {
      set({ error: String(e) });
      throw e;
    }
  },

  removeRepo: async (repoId) => {
    set({ error: null });
    try {
      await invoke("remove_repo", { repoId });
      const repos = await invoke<RepoWithSessions[]>("list_repos");
      const { activeSessionId } = get();
      if (activeSessionId !== null) {
        const stillExists = repos.some((r) =>
          r.sessions.some((s) => s.id === activeSessionId),
        );
        set({
          repos,
          activeSessionId: stillExists ? activeSessionId : null,
        });
      } else {
        set({ repos });
      }
    } catch (e) {
      set({ error: String(e) });
    }
  },

  createSession: async (repoId, useWorktree, branch) => {
    set({ error: null });
    try {
      const session = await invoke<Session>("create_session", {
        repoId,
        useWorktree,
        branch: branch || null,
      });
      const repos = await invoke<RepoWithSessions[]>("list_repos");
      set({ repos, activeSessionId: session.id });
    } catch (e) {
      set({ error: String(e) });
      throw e;
    }
  },

  stopSession: async (sessionId) => {
    try {
      await invoke("stop_session", { sessionId });
      const repos = await invoke<RepoWithSessions[]>("list_repos");
      const { activeSessionId } = get();
      set({
        repos,
        activeSessionId:
          activeSessionId === sessionId ? null : activeSessionId,
      });
    } catch (e) {
      set({ error: String(e) });
    }
  },

  removeSession: async (sessionId) => {
    try {
      await invoke("remove_session", { sessionId });
      const repos = await invoke<RepoWithSessions[]>("list_repos");
      const { activeSessionId } = get();
      set({
        repos,
        activeSessionId:
          activeSessionId === sessionId ? null : activeSessionId,
      });
    } catch (e) {
      set({ error: String(e) });
    }
  },

  setActiveSession: (id) => set({ activeSessionId: id }),

  clearError: () => set({ error: null }),
}));
