import { useEffect } from "react";
import { Sidebar } from "./components/Sidebar/Sidebar";
import { Terminal } from "./components/Terminal/Terminal";
import { InsightsPanel } from "./components/Insights/InsightsPanel";
import { StatusBar } from "./components/Dashboard/StatusBar";
import { ActivityPanel } from "./components/ActivityPanel/ActivityPanel";
import { FileViewer } from "./components/FileViewer/FileViewer";
import { CommandPalette } from "./components/FileViewer/CommandPalette";
import { useSessionStore } from "./stores/sessionStore";
import { useFileViewerStore } from "./stores/fileViewerStore";

function App() {
  const initialize = useSessionStore((s) => s.initialize);

  useEffect(() => {
    initialize();
  }, [initialize]);

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

  return (
    <div className="flex h-screen flex-col bg-surface-0">
      {/* Main Content */}
      <div className="flex flex-1 min-h-0 overflow-hidden">
        {/* Left Sidebar — Session List (~15%) */}
        <Sidebar />

        {/* Center — Agent Terminal (~55%) */}
        <main className="relative flex flex-1 flex-col border-x border-surface-3">
          <Terminal />
          <FileViewer />
        </main>

        {/* Right Panel — Insights (~30%) */}
        <aside className="flex w-80 flex-col overflow-hidden">
          <InsightsPanel />
        </aside>
      </div>

      {/* Activity Panel — session activity monitor */}
      <ActivityPanel />

      {/* Global Status Bar */}
      <StatusBar />
      <CommandPalette />
    </div>
  );
}

export default App;
