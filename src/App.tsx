import { useEffect, useState, useRef } from "react";
import { listen } from "@tauri-apps/api/event";
import { Sidebar } from "./components/Sidebar/Sidebar";
import { Terminal } from "./components/Terminal/Terminal";
import { ResetDbDialog } from "./components/Sidebar/ResetDbDialog";

import { StatusBar } from "./components/Dashboard/StatusBar";
import { FileViewer } from "./components/FileViewer/FileViewer";
import { CommandPalette } from "./components/FileViewer/CommandPalette";
import { TaskBoard } from "./components/TaskBoard/TaskBoard";
import { useSessionStore } from "./stores/sessionStore";
import { useFileViewerStore } from "./stores/fileViewerStore";
import { useTaskStore } from "./stores/taskStore";

function App() {
  const initialize = useSessionStore((s) => s.initialize);
  const [centerTab, setCenterTab] = useState<"tasks" | "terminal">("tasks");
  const activeSessionId = useSessionStore((s) => s.activeSessionId);
  const repos = useSessionStore((s) => s.repos);
  const tasks = useTaskStore((s) => s.tasks);
  const loadTasks = useTaskStore((s) => s.loadTasks);

  // Find the repo of the active session, or first repo
  const activeRepoId =
    repos.find((r) => r.sessions.some((s) => s.id === activeSessionId))?.repo
      .id ??
    repos[0]?.repo.id ??
    null;

  useEffect(() => {
    initialize();
  }, [initialize]);

  // Load tasks at App level so tab badge works before TaskBoard mounts
  useEffect(() => {
    if (activeRepoId) loadTasks(activeRepoId);
  }, [activeRepoId, loadTasks]);

  // Switch to terminal when user clicks a session in sidebar.
  // Skip initial mount and skip when fireTask sets activeSessionId.
  const prevSessionRef = useRef(activeSessionId);
  useEffect(() => {
    if (
      activeSessionId &&
      activeSessionId !== prevSessionRef.current
    ) {
      // Don't auto-switch if fireTask just created this session
      const skip = useSessionStore.getState()._skipTerminalSwitch;
      if (skip) {
        useSessionStore.setState({ _skipTerminalSwitch: false });
      } else {
        setCenterTab("terminal");
      }
    }
    prevSessionRef.current = activeSessionId;
  }, [activeSessionId]);

  const [resetDialogOpen, setResetDialogOpen] = useState(false);

  useEffect(() => {
    const unlisten = listen("menu-reset-db", () => {
      setResetDialogOpen(true);
    });
    return () => { unlisten.then((fn) => fn()); };
  }, []);

  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key === "p") {
        e.preventDefault();
        useFileViewerStore.getState().openPalette();
      }
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, []);

  const activeTaskCount = tasks.filter((t) => t.status !== "closed").length;

  return (
    <div className="flex h-screen flex-col bg-surface-0">
      {/* Main Content */}
      <div className="flex flex-1 min-h-0 overflow-hidden">
        {/* Left Sidebar — Session List (~15%) */}
        <Sidebar onNewTask={() => {
          setCenterTab("tasks");
          useTaskStore.getState().setDraftInputOpen(true);
          useTaskStore.getState().setDraftValue("");
        }} onSessionSelect={() => setCenterTab("terminal")} />

        {/* Center — Tasks / Terminal (~55%) */}
        <main className="relative flex flex-1 flex-col border-x border-surface-3">
          {/* Tab bar */}
          <div className="flex border-b border-surface-3 bg-surface-1">
            <button
              onClick={() => setCenterTab("tasks")}
              className={`px-4 py-2 text-xs uppercase tracking-wider transition-colors ${
                centerTab === "tasks"
                  ? "border-b-2 border-accent text-zinc-200"
                  : "text-zinc-500 hover:text-zinc-300"
              }`}
            >
              Tasks
              {activeTaskCount > 0 && (
                <span
                  className={`ml-2 rounded-full px-1.5 py-0.5 text-[9px] ${
                    centerTab === "tasks"
                      ? "bg-accent/20 text-accent"
                      : "bg-surface-3 text-zinc-500"
                  }`}
                >
                  {activeTaskCount}
                </span>
              )}
            </button>
            <button
              onClick={() => setCenterTab("terminal")}
              className={`px-4 py-2 text-xs uppercase tracking-wider transition-colors ${
                centerTab === "terminal"
                  ? "border-b-2 border-accent text-zinc-200"
                  : "text-zinc-500 hover:text-zinc-300"
              }`}
            >
              Terminal
            </button>
          </div>

          {/* Content — Terminal stays mounted to preserve xterm.js state */}
          {centerTab === "tasks" && (
            <TaskBoard repoId={activeRepoId} onSessionSelect={() => setCenterTab("terminal")} />
          )}
          <div className={centerTab === "terminal" ? "flex flex-1 flex-col" : "hidden"}>
            <Terminal />
            <FileViewer />
          </div>
        </main>


      </div>

      {/* Global Status Bar */}
      <StatusBar />
      <CommandPalette />
      <ResetDbDialog
        open={resetDialogOpen}
        onClose={() => setResetDialogOpen(false)}
      />
    </div>
  );
}

export default App;
