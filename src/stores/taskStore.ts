import { create } from "zustand";
import { invoke } from "@tauri-apps/api/core";
import type { Task, TaskStatus } from "../types/task";

interface TaskState {
  tasks: Task[];
  loading: boolean;
  error: string | null;

  loadTasks: (repoId: number) => Promise<void>;
  createTask: (repoId: number, description: string) => Promise<Task>;
  fireTask: (
    taskId: number,
    repoId: number,
    useWorktree: boolean,
    branch: string | undefined,
    skipPermissions: boolean
  ) => Promise<void>;
  updateTaskStatus: (
    taskId: number,
    status: TaskStatus,
    sessionId?: number
  ) => Promise<void>;
  deleteTask: (taskId: number) => Promise<void>;
  syncTaskWithSession: (sessionId: number, sessionStatus: string) => void;
}

export const useTaskStore = create<TaskState>((set, get) => ({
  tasks: [],
  loading: false,
  error: null,

  loadTasks: async (repoId: number) => {
    set({ loading: true, error: null });
    try {
      const tasks = await invoke<Task[]>("list_tasks", { repoId });
      set({ tasks, loading: false });
    } catch (err) {
      set({ error: String(err), loading: false });
    }
  },

  createTask: async (repoId: number, description: string) => {
    try {
      const task = await invoke<Task>("create_task", { repoId, description });
      set((state: TaskState) => ({ tasks: [task, ...state.tasks] }));
      return task;
    } catch (err) {
      set({ error: String(err) });
      throw err;
    }
  },

  // NOTE: The spec lists fire_task as a Rust command, but per the spec's own
  // Implementation Notes, agent/skip-permissions are frontend concerns.
  // fire_task is intentionally implemented on the frontend, calling the
  // existing create_session Rust command internally.
  fireTask: async (taskId: number, repoId: number, useWorktree: boolean, branch: string | undefined, skipPermissions: boolean) => {
    // Import sessionStore dynamically to avoid circular deps
    const { useSessionStore } = await import("./sessionStore");
    const { createSession } = useSessionStore.getState();

    // Capture session count before creation to find the new one
    const beforeRepos = useSessionStore.getState().repos;
    const beforeRepo = beforeRepos.find((r) => r.repo.id === repoId);
    const beforeIds = new Set(beforeRepo?.sessions.map((s) => s.id) ?? []);

    // Create session via existing flow
    await createSession(repoId, useWorktree, branch, skipPermissions);

    // Find the newly created session by diffing session IDs
    const afterRepos = useSessionStore.getState().repos;
    const afterRepo = afterRepos.find((r) => r.repo.id === repoId);
    const newSession = afterRepo?.sessions.find((s) => !beforeIds.has(s.id));

    if (!newSession) {
      set({ error: "fireTask: could not identify newly created session" });
      return;
    }

    // Link task to session and set running
    await get().updateTaskStatus(taskId, "running", newSession.id);

    // Send task description to PTY as initial prompt.
    // Uses a 2s delay as pragmatic fallback for agent initialization.
    // Known limitation: if agent takes longer (cold start, large repo),
    // the prompt may arrive too early. A signal-based approach would be
    // more robust but is deferred to keep scope minimal.
    const task = get().tasks.find((t: Task) => t.id === taskId);
    if (task) {
      const { writePty } = await import("../services/ptyManager");
      setTimeout(() => {
        writePty(newSession.id, task.description + "\n");
      }, 2000);
    }
  },

  updateTaskStatus: async (taskId: number, status: TaskStatus, sessionId?: number) => {
    try {
      const task = await invoke<Task>("update_task_status", {
        taskId,
        status,
        sessionId: sessionId ?? null,
      });
      set((state: TaskState) => ({
        tasks: state.tasks.map((t: Task) => (t.id === taskId ? task : t)),
      }));
    } catch (err) {
      set({ error: String(err) });
      throw err;
    }
  },

  deleteTask: async (taskId: number) => {
    try {
      await invoke("delete_task", { taskId });
      set((state: TaskState) => ({
        tasks: state.tasks.filter((t: Task) => t.id !== taskId),
      }));
    } catch (err) {
      set({ error: String(err) });
      throw err;
    }
  },

  syncTaskWithSession: (sessionId: number, sessionStatus: string) => {
    const { tasks } = get();
    const task = tasks.find(
      (t: Task) => t.session_id === sessionId && t.status === "running"
    );
    if (task && sessionStatus === "Completed") {
      get()
        .updateTaskStatus(task.id, "review")
        .catch((err) => set({ error: String(err) }));
    }
  },
}));
