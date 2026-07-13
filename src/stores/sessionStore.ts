import { create } from "zustand";
import { transport } from "../services/transport";
import type { Repo, Session, RepoWithSessions, SessionStatus } from "../types/session";
import { startTracking, stopTracking, setOutputCallback, setPrUrlCallback } from "../services/ptyOutputParser";

interface SessionState {
  repos: RepoWithSessions[];
  activeSessionId: number | null;
  /** When true, the next activeSessionId change should NOT auto-switch to terminal tab. */
  _skipTerminalSwitch: boolean;
  loading: boolean;
  error: string | null;

  sessionLastOutput: Record<number, string>;

  /** Bumped per session each time its transport is silently re-attached, so the
   *  terminal can re-sync state (a reconnected PTY starts at the default size). */
  reconnectNonce: Record<number, number>;

  updateSessionLastOutput: (sessionId: number, line: string) => void;
  clearSessionLastOutput: (sessionId: number) => void;
  updateSessionPrUrl: (sessionId: number, prUrl: string) => void;
  updateSessionStatus: (sessionId: number, status: SessionStatus) => void;

  getActiveSession: () => { session: Session; repo: Repo } | null;

  initialize: () => Promise<void>;
  refreshRepos: (trackSessionId?: number) => Promise<void>;
  importRepo: (path: string) => Promise<void>;
  removeRepo: (repoId: number) => Promise<void>;
  createSession: (
    repoId: number,
    useWorktree: boolean,
    branch?: string,
    skipPermissions?: boolean,
    serverId?: string,
    taskDescription?: string,
  ) => Promise<void>;
  reattachSession: (sessionId: number, skipPermissions?: boolean) => Promise<void>;
  openSession: (sessionId: number) => Promise<void>;
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
  reconnectNonce: {},

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
      transport.call("update_session_pr_url", { sessionId, prUrl }).then(() => {
        get().updateSessionPrUrl(sessionId, prUrl);
      }).catch((e) => console.warn("[sessionStore] Failed to save PR URL:", e));

      // Send system notification
      try {
        if (transport.isLocal()) {
          import("@tauri-apps/plugin-notification").then((m) =>
            m.sendNotification({
              title: "New PR Created",
              body: `${current?.branch ?? "Session"} — ${prUrl}`,
            })
          );
        } else if ("Notification" in window) {
          new Notification("New PR Created", {
            body: `${current?.branch ?? "Session"} — ${prUrl}`,
          });
        }
      } catch (e) {
        console.warn("[sessionStore] Failed to send notification:", e);
      }
    });


    set({ loading: true, error: null });
    try {
      const repos = await transport.call("reconcile_sessions") as RepoWithSessions[];
      set({ repos, loading: false });
    } catch (e) {
      set({ repos: [], loading: false, error: String(e) });
    }

    // Listen for remotely-created sessions (from WebSocket API)
    transport.on('racc://session-created', async (data: {
      session_id: number;
      repo_id: number;
      branch: string | null;
      worktree_path: string;
      agent: string;
      source: string;
      reattach?: boolean;
    }) => {
      const { session_id, source } = data;
      if (source !== 'remote') return;

      // Refresh session list from DB
      const repos = await transport.call("list_repos") as RepoWithSessions[];
      set({ repos });

      // PTY is already spawned by Rust-side create_session, just start tracking output
      startTracking(session_id);
    });

    // Listen for remotely-stopped sessions
    transport.on('racc://session-stopped', async (data: {
      session_id: number;
      source: string;
    }) => {
      const { session_id, source } = data;
      if (source !== 'remote') return;

      stopTracking(session_id);
      const repos = await transport.call("list_repos") as RepoWithSessions[];
      set({ repos });
    });

    // Live session-status updates from the backend — e.g. the resume watcher
    // flipping a session to Error when `claude --resume` finds no conversation
    // (issue #70). This lands 1-2s AFTER reattach_session resolves, so only a
    // push (not the post-reattach list refresh) can surface it. Tauri wraps
    // RaccEvents as {event, data} on "racc://event"; browser mode delivers
    // them unwrapped, keyed by event name.
    const applyStatusChange = (data: { session_id: number; status: string }) => {
      if (typeof data?.session_id !== "number") return;
      get().updateSessionStatus(data.session_id, data.status as SessionStatus);
    };
    transport.on("session_status_changed", applyStatusChange);
    transport.on("racc://event", (evt: { event: string; data: any }) => {
      if (evt?.event === "session_status_changed") applyStatusChange(evt.data);
    });
  },

  refreshRepos: async (trackSessionId) => {
    const repos = await transport.call("list_repos") as RepoWithSessions[];
    set({ repos });
    if (trackSessionId != null) startTracking(trackSessionId);
  },

  importRepo: async (path) => {
    set({ error: null });
    try {
      await transport.call("import_repo", { path }) as Repo;
      const repos = await transport.call("list_repos") as RepoWithSessions[];
      set({ repos });
    } catch (e) {
      set({ error: String(e) });
      throw e;
    }
  },

  removeRepo: async (repoId) => {
    set({ error: null });
    try {
      await transport.call("remove_repo", { repoId });
      const repos = await transport.call("list_repos") as RepoWithSessions[];
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

  createSession: async (repoId, useWorktree, branch, skipPermissions = true, serverId, taskDescription) => {
    set({ error: null });
    try {
      // PTY is now spawned by Rust-side create_session
      const session = await transport.call("create_session", {
        repoId,
        useWorktree,
        branch: branch || null,
        agent: "claude-code",
        taskDescription: taskDescription || null,
        serverId: serverId || null,
        skipPermissions,
      }) as Session;

      // Start tracking PTY output via transport:data events
      startTracking(session.id);

      const updatedRepos = await transport.call("list_repos") as RepoWithSessions[];
      set({ repos: updatedRepos, activeSessionId: session.id });
    } catch (e) {
      set({ error: String(e) });
      throw e;
    }
  },

  reattachSession: async (sessionId, _skipPermissions = true) => {
    set({ error: null });
    try {
      const session = await transport.call("reattach_session", { sessionId }) as Session;

      // Start tracking PTY output via transport:data events
      startTracking(session.id);

      const updatedRepos = await transport.call("list_repos") as RepoWithSessions[];
      set({ repos: updatedRepos, activeSessionId: session.id });
    } catch (e) {
      set({ error: String(e) });
      throw e;
    }
  },

  // Open a session for viewing. Selects it, then silently re-establishes its
  // transport if it isn't live. This covers two cases that would otherwise show
  // a frozen/blank terminal: (1) the SSH connection dropped while the laptop
  // slept — the remote tmux is still running, so we just re-attach to it; and
  // (2) after an app restart a remote session is "Running" in the DB but its
  // in-memory transport was lost. `reconnect_session` is idempotent and never
  // tears down a live session, so it's safe to call on every open.
  openSession: async (sessionId) => {
    set({ activeSessionId: sessionId });
    try {
      const outcome = await transport.call("reconnect_session", { sessionId }) as
        | "AlreadyLive"
        | "Reconnected"
        | "FullReattach"
        | "Gone";
      if (outcome === "FullReattach") {
        // Local session (or one needing `claude --continue`) — heavier path.
        await get().reattachSession(sessionId);
        return; // reattachSession already refreshes the repo list
      }
      if (outcome === "Reconnected") {
        // Re-arm output parsing for the freshly re-attached transport, and bump
        // the nonce so the terminal re-sends its size (the new PTY defaults to
        // 80x24, which would otherwise clamp the tmux window).
        startTracking(sessionId);
        set((s) => ({
          reconnectNonce: {
            ...s.reconnectNonce,
            [sessionId]: (s.reconnectNonce[sessionId] ?? 0) + 1,
          },
        }));
      }
      if (outcome === "Reconnected" || outcome === "Gone") {
        // Reflect the status change (Running re-attach, or Completed if gone).
        const repos = await transport.call("list_repos") as RepoWithSessions[];
        set({ repos });
      }
      // AlreadyLive: nothing to do.
    } catch (e) {
      // A passive click shouldn't surface a scary error if the server is
      // momentarily unreachable; leave the terminal as-is and log it.
      console.warn("[sessionStore] reconnect on open failed:", e);
    }
  },

  stopSession: async (sessionId) => {
    try {
      stopTracking(sessionId);
      // Transport is closed by Rust-side stop_session
      await transport.call("stop_session", { sessionId });

      // Trigger batch analysis when a session ends
      transport.call("run_batch_analysis").catch(() => {});

      const repos = await transport.call("list_repos") as RepoWithSessions[];
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
      // Transport is closed by Rust-side remove_session
      await transport.call("remove_session", { sessionId, deleteWorktree });

      // Trigger batch analysis (session may have been running)
      transport.call("run_batch_analysis").catch(() => {});

      const repos = await transport.call("list_repos") as RepoWithSessions[];
      const { activeSessionId } = get();
      set({
        repos,
        activeSessionId:
          activeSessionId === sessionId ? null : activeSessionId,
      });
    } catch (e) {
      set({ error: String(e) });
      // Re-throw so RemoveSessionDialog can surface the failure instead of
      // silently closing while the session stays in the list.
      throw e;
    }
  },

  setActiveSession: (id) => set({ activeSessionId: id }),

  resetDb: async () => {
    set({ error: null });
    try {
      await transport.call("reset_db");
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

  updateSessionStatus: (sessionId, status) => {
    set({
      repos: get().repos.map((rws) => ({
        ...rws,
        sessions: rws.sessions.map((s) =>
          s.id === sessionId ? { ...s, status } : s,
        ),
      })),
    });
  },
}));
