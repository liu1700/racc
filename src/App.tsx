import { useEffect } from "react";
import { Sidebar } from "./components/Sidebar/Sidebar";
import { Terminal } from "./components/Terminal/Terminal";
import { ActivityLog } from "./components/ActivityLog/ActivityLog";
import { CostTracker } from "./components/CostTracker/CostTracker";
import { StatusBar } from "./components/Dashboard/StatusBar";
import { useSessionStore } from "./stores/sessionStore";

function App() {
  const fetchSessions = useSessionStore((s) => s.fetchSessions);

  // Fetch existing sessions on mount and periodically refresh
  useEffect(() => {
    fetchSessions();
    const interval = setInterval(fetchSessions, 5000);
    return () => clearInterval(interval);
  }, [fetchSessions]);

  return (
    <div className="flex h-screen flex-col bg-surface-0">
      {/* Main Content */}
      <div className="flex flex-1 overflow-hidden">
        {/* Left Sidebar — Session List (~15%) */}
        <Sidebar />

        {/* Center — Agent Terminal (~55%) */}
        <main className="flex flex-1 flex-col border-x border-surface-3">
          <Terminal />
        </main>

        {/* Right Panel — Activity + Cost (~30%) */}
        <aside className="flex w-80 flex-col overflow-hidden">
          <CostTracker />
          <ActivityLog />
        </aside>
      </div>

      {/* Global Status Bar */}
      <StatusBar />
    </div>
  );
}

export default App;
