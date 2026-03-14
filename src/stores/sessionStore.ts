import { create } from "zustand";
import { invoke } from "@tauri-apps/api/core";
import type { Repo, Session, RepoWithSessions } from "../types/session";
import { startTracking, stopTracking, setOutputCallback, setPrUrlCallback } from "../services/ptyOutputParser";
import { sendNotification } from "@tauri-apps/plugin-notification";
import { spawnPty, killPty, killAll } from "../services/ptyManager";

interface SessionState {
  repos: RepoWithSessions[];
  activeSessionId: number | null;
  /** When true, the next activeSessionId change should NOT auto-switch to terminal tab. */
  _skipTerminalSwitch: boolean;
  loading: boolean;
  error: string | null;

  sessionLastOutput: Record<number, string>;

  updateSessionLastOutput: (sessionId: number, line: string) => void;
  clearSessionLastOutput: (sessionId: number) => void;
  updateSessionPrUrl: (sessionId: number, prUrl: string) => void;

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
  resetDb: () => Promise<void>;
  clearError: () => void;
}

export const useSessionStore = create<SessionState>((set, get) => ({
  repos: [],
  activeSessionId: null,
  _skipTerminalSwitch: false,
  loading: false,
  error: null,

  sessionLastOutput: {},

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
    // Wire up the PTY output callback
    setOutputCallback((sessionId, lastLine) => {
      get().updateSessionLastOutput(sessionId, lastLine);
    });

    // Wire up PR URL detection callback
    setPrUrlCallback((sessionId, prUrl) => {
      // Check if pr_url changed to avoid redundant DB writes
      const current = get().repos
        .flatMap((r) => r.sessions)
        .find((s) => s.id === sessionId);
      if (current?.pr_url === prUrl) return;

      // Persist to DB then update local state
      invoke("update_session_pr_url", { sessionId, prUrl }).then(() => {
        get().updateSessionPrUrl(sessionId, prUrl);
      }).catch((e) => console.warn("[sessionStore] Failed to save PR URL:", e));

      // Send system notification
      try {
        sendNotification({
          title: "New PR Created",
          body: `${current?.branch ?? "Session"} — ${prUrl}`,
        });
      } catch (e) {
        console.warn("[sessionStore] Failed to send notification:", e);
      }
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

      // Spawn PTY in the session's working directory
      spawnPty(session.id, cwd, 80, 24, agentCmd);

      // Start tracking PTY output
      startTracking(session.id);

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

      spawnPty(session.id, cwd, 80, 24, agentCmd);

      startTracking(session.id);

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
      killPty(sessionId);
      await invoke("stop_session", { sessionId });

      // Trigger batch analysis when a session ends
      invoke("run_batch_analysis").catch(() => {});

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
      get().clearSessionLastOutput(sessionId);
      killPty(sessionId);
      await invoke("remove_session", { sessionId, deleteWorktree });

      // Trigger batch analysis (session may have been running)
      invoke("run_batch_analysis").catch(() => {});

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

  resetDb: async () => {
    set({ error: null });
    try {
      killAll();
      await invoke("reset_db");
      set({
        repos: [],
        activeSessionId: null,
        sessionLastOutput: {},
      });
    } catch (e) {
      set({ error: String(e) });
      throw e;
    }
  },

  clearError: () => set({ error: null }),

  updateSessionLastOutput: (sessionId, line) => {
    set({
      sessionLastOutput: { ...get().sessionLastOutput, [sessionId]: line },
    });
  },

  clearSessionLastOutput: (sessionId) => {
    const { [sessionId]: _, ...rest } = get().sessionLastOutput;
    set({ sessionLastOutput: rest });
  },

  updateSessionPrUrl: (sessionId, prUrl) => {
    set({
      repos: get().repos.map((rws) => ({
        ...rws,
        sessions: rws.sessions.map((s) =>
          s.id === sessionId ? { ...s, pr_url: prUrl } : s,
        ),
      })),
    });
  },
}));
