import { create } from "zustand";
import { transport } from "../services/transport";
import type { Task, TaskStatus, DraftImage } from "../types/task";

function parseTask(raw: Record<string, unknown>): Task {
  return {
    ...(raw as unknown as Task),
    images:
      typeof raw.images === "string"
        ? JSON.parse(raw.images as string)
        : (raw.images as string[]) ?? [],
  };
}

interface TaskState {
  tasks: Task[];
  loading: boolean;
  error: string | null;
  draftInputOpen: boolean;
  draftValue: string;
  draftImages: DraftImage[];

  setDraftInputOpen: (open: boolean) => void;
  setDraftValue: (value: string) => void;
  addDraftImage: (image: DraftImage) => void;
  removeDraftImage: (filename: string) => void;
  clearDraftImages: () => void;
  loadTasks: (repoId: number) => Promise<void>;
  createTask: (
    repoId: number,
    description: string,
    images?: string[]
  ) => Promise<Task>;
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
  updateTaskDescription: (taskId: number, description: string) => Promise<void>;
  deleteTask: (taskId: number) => Promise<void>;
  syncTaskWithSession: (sessionId: number, sessionStatus: string) => void;
}

export const useTaskStore = create<TaskState>((set, get) => ({
  tasks: [],
  loading: false,
  error: null,
  draftInputOpen: false,
  draftValue: "",
  draftImages: [],

  setDraftInputOpen: (open: boolean) => set({ draftInputOpen: open }),
  setDraftValue: (value: string) => set({ draftValue: value }),
  addDraftImage: (image: DraftImage) =>
    set((state) => ({ draftImages: [...state.draftImages, image] })),
  removeDraftImage: (filename: string) =>
    set((state) => ({
      draftImages: state.draftImages.filter((i) => i.filename !== filename),
    })),
  clearDraftImages: () => {
    const { draftImages } = get();
    for (const img of draftImages) {
      URL.revokeObjectURL(img.objectUrl);
    }
    set({ draftImages: [] });
  },

  loadTasks: async (repoId: number) => {
    set({ loading: true, error: null });
    try {
      const raw = await transport.call("list_tasks", {
        repoId,
      }) as Record<string, unknown>[];
      const tasks = raw.map(parseTask);
      set({ tasks, loading: false });
    } catch (err) {
      set({ error: String(err), loading: false });
    }
  },

  createTask: async (
    repoId: number,
    description: string,
    images: string[] = []
  ) => {
    try {
      const raw = await transport.call("create_task", {
        repoId,
        description,
        images: JSON.stringify(images),
      }) as Record<string, unknown>;
      const task = parseTask(raw);
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
  fireTask: async (
    taskId: number,
    repoId: number,
    useWorktree: boolean,
    branch: string | undefined,
    skipPermissions: boolean
  ) => {
    // Import sessionStore dynamically to avoid circular deps
    const { useSessionStore } = await import("./sessionStore");
    const { createSession } = useSessionStore.getState();

    // Build the task prompt (description + optional image paths)
    const task = get().tasks.find((t: Task) => t.id === taskId);
    if (!task) {
      set({ error: "fireTask: task not found" });
      return;
    }

    let prompt = task.description;
    if (task.images.length > 0) {
      const repo = useSessionStore
        .getState()
        .repos.find((r) => r.repo.id === task.repo_id);
      if (repo) {
        const imagePaths = task.images.map(
          (img) => `${repo.repo.path}/.racc/images/${img}`
        );
        prompt +=
          "\n\nRefer to the following images:\n" +
          imagePaths.map((p) => `- ${p}`).join("\n");
      }
    }

    // Capture session count before creation to find the new one
    const beforeRepos = useSessionStore.getState().repos;
    const beforeRepo = beforeRepos.find((r) => r.repo.id === repoId);
    const beforeIds = new Set(beforeRepo?.sessions.map((s) => s.id) ?? []);

    // Tell sessionStore not to auto-switch to terminal tab
    useSessionStore.setState({ _skipTerminalSwitch: true });

    // Create session with task description — Rust builds the `claude '...'` command
    await createSession(repoId, useWorktree, branch, skipPermissions, undefined, prompt);

    // Find the newly created session by diffing session IDs
    const afterRepos = useSessionStore.getState().repos;
    const afterRepo = afterRepos.find((r) => r.repo.id === repoId);
    const newSession = afterRepo?.sessions.find((s) => !beforeIds.has(s.id));

    if (!newSession) {
      set({ error: "fireTask: could not identify newly created session" });
      return;
    }

    // Link task to session and set running
    await get().updateTaskStatus(taskId, "working", newSession.id);
  },

  updateTaskStatus: async (
    taskId: number,
    status: TaskStatus,
    sessionId?: number
  ) => {
    try {
      const raw = await transport.call("update_task_status", {
        taskId,
        status,
        sessionId: sessionId ?? null,
      }) as Record<string, unknown>;
      const task = parseTask(raw);
      set((state: TaskState) => ({
        tasks: state.tasks.map((t: Task) => (t.id === taskId ? task : t)),
      }));
    } catch (err) {
      set({ error: String(err) });
      throw err;
    }
  },

  updateTaskDescription: async (taskId: number, description: string) => {
    try {
      const raw = await transport.call(
        "update_task_description",
        {
          taskId,
          description,
        }
      ) as Record<string, unknown>;
      const task = parseTask(raw);
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
      await transport.call("delete_task", { taskId });
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
      (t: Task) => t.session_id === sessionId && t.status === "working"
    );
    if (task && sessionStatus === "Completed") {
      get()
        .updateTaskStatus(task.id, "closed")
        .catch((err) => set({ error: String(err) }));
    }
  },
}));
