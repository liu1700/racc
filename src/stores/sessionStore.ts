import { create } from "zustand";
import { invoke } from "@tauri-apps/api/core";
import type { Repo, Session, RepoWithSessions, SessionActivity } from "../types/session";
import { startTracking, stopTracking, setActivityCallback } from "../services/ptyOutputParser";
import { spawnPty, killPty, killAll } from "../services/ptyManager";

interface SessionState {
  repos: RepoWithSessions[];
  activeSessionId: number | null;
  loading: boolean;
  error: string | null;

  sessionActivities: Record<number, SessionActivity>;
  activityPanelOpen: boolean;
  activityPanelDismissed: boolean;

  updateSessionActivity: (sessionId: number, activity: SessionActivity) => void;
  removeSessionActivity: (sessionId: number) => void;
  setActivityPanelOpen: (open: boolean) => void;
  dismissActivityPanel: () => void;

  getActiveSession: () => { session: Session; repo: Repo } | null;

  initialize: () => Promise<void>;
  importRepo: (path: string) => Promise<void>;
  removeRepo: (repoId: number) => Promise<void>;
  createSession: (
    repoId: number,
    useWorktree: boolean,
    branch?: string,
    skipPermissions?: boolean,
  ) => Promise<void>;
  reattachSession: (sessionId: number, skipPermissions?: boolean) => Promise<void>;
  stopSession: (sessionId: number) => Promise<void>;
  removeSession: (sessionId: number, deleteWorktree?: boolean) => Promise<void>;
  setActiveSession: (id: number) => void;
  clearError: () => void;
}

export const useSessionStore = create<SessionState>((set, get) => ({
  repos: [],
  activeSessionId: null,
  loading: false,
  error: null,

  sessionActivities: {},
  activityPanelOpen: false,
  activityPanelDismissed: false,

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
    // Wire up the PTY output parser callback
    setActivityCallback((sessionId, activity) => {
      get().updateSessionActivity(sessionId, activity);
    });

    set({ loading: true, error: null });
    try {
      const repos = await invoke<RepoWithSessions[]>("reconcile_sessions");
      set({ repos, loading: false });
    } catch (e) {
      set({ repos: [], loading: false, error: String(e) });
    }

    // Kill all PTYs on app close to avoid orphaned processes
    window.addEventListener("beforeunload", () => killAll());
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

  createSession: async (repoId, useWorktree, branch, skipPermissions = true) => {
    set({ error: null });
    try {
      const session = await invoke<Session>("create_session", {
        repoId,
        useWorktree,
        branch: branch || null,
      });

      // Resolve working directory: worktree path, or fall back to repo path
      const { repos } = get();
      const repo = repos.find((r) => r.repo.id === repoId)?.repo;
      const cwd = session.worktree_path || repo?.path || ".";

      // Build agent command with optional flags
      const agentCmd = skipPermissions ? "claude --dangerously-skip-permissions" : "claude";

      // Reset panel dismissed state if this is the first running session
      const runningSessions = get().repos.flatMap((r) => r.sessions).filter((s) => s.status === "Running");
      if (runningSessions.length === 0) {
        set({ activityPanelDismissed: false });
      }

      // Spawn PTY in the session's working directory
      spawnPty(session.id, cwd, 80, 24, agentCmd);

      // Start tracking PTY output for activity panel
      startTracking(session.id, session.agent);

      const updatedRepos = await invoke<RepoWithSessions[]>("list_repos");
      set({ repos: updatedRepos, activeSessionId: session.id });
    } catch (e) {
      set({ error: String(e) });
      throw e;
    }
  },

  reattachSession: async (sessionId, skipPermissions = true) => {
    set({ error: null });
    try {
      const session = await invoke<Session>("reattach_session", { sessionId });

      const { repos } = get();
      const repo = repos.find((r) => r.repo.id === session.repo_id)?.repo;
      const cwd = session.worktree_path || repo?.path || ".";

      const flags = skipPermissions ? " --dangerously-skip-permissions" : "";
      const agentCmd = `claude --continue${flags}`;

      // Reset panel dismissed state if this is the first running session
      const runningSessions = get().repos.flatMap((r) => r.sessions).filter((s) => s.status === "Running");
      if (runningSessions.length === 0) {
        set({ activityPanelDismissed: false });
      }

      spawnPty(session.id, cwd, 80, 24, agentCmd);

      startTracking(session.id, session.agent);

      const updatedRepos = await invoke<RepoWithSessions[]>("list_repos");
      set({ repos: updatedRepos, activeSessionId: session.id });
    } catch (e) {
      set({ error: String(e) });
      throw e;
    }
  },

  stopSession: async (sessionId) => {
    try {
      stopTracking(sessionId);
      // Update activity to show completion before removing
      get().updateSessionActivity(sessionId, {
        sessionId,
        action: "Completed",
        detail: null,
        timestamp: Date.now(),
      });
      killPty(sessionId);
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

  removeSession: async (sessionId, deleteWorktree = false) => {
    try {
      stopTracking(sessionId);
      get().removeSessionActivity(sessionId);
      killPty(sessionId);
      await invoke("remove_session", { sessionId, deleteWorktree });
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

  updateSessionActivity: (sessionId, activity) => {
    const current = get().sessionActivities[sessionId];
    // De-duplicate: skip set() if action + detail unchanged
    if (current && current.action === activity.action && current.detail === activity.detail) {
      return;
    }
    const { activityPanelOpen, activityPanelDismissed } = get();
    set({
      sessionActivities: { ...get().sessionActivities, [sessionId]: activity },
      // Auto-open panel if not user-dismissed
      ...(!activityPanelOpen && !activityPanelDismissed ? { activityPanelOpen: true } : {}),
    });
  },

  removeSessionActivity: (sessionId) => {
    const { [sessionId]: _, ...rest } = get().sessionActivities;
    const hasRemaining = Object.keys(rest).length > 0;
    set({
      sessionActivities: rest,
      // Auto-close when no remaining activities
      ...(!hasRemaining ? { activityPanelOpen: false } : {}),
    });
  },

  setActivityPanelOpen: (open) => set({ activityPanelOpen: open }),

  dismissActivityPanel: () =>
    set({ activityPanelOpen: false, activityPanelDismissed: true }),
}));
