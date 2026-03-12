import { useSessionStore } from "../../stores/sessionStore";
import type { SessionStatus } from "../../types/session";

const statusColor: Record<SessionStatus, string> = {
  Creating: "bg-blue-400",
  Running: "bg-status-running",
  Waiting: "bg-status-waiting",
  Paused: "bg-status-paused",
  Disconnected: "bg-status-disconnected",
  Completed: "bg-status-completed",
  Error: "bg-status-error",
};

export function Sidebar() {
  const { sessions, activeSessionId, setActiveSession, stopSession } =
    useSessionStore();

  return (
    <aside className="flex w-56 flex-col bg-surface-1">
      {/* Header */}
      <div className="flex items-center justify-between border-b border-surface-3 px-4 py-3">
        <h1 className="text-sm font-semibold tracking-wide text-zinc-200">
          OTTE
        </h1>
        <button className="rounded bg-accent px-2 py-1 text-xs font-medium text-white hover:bg-accent-hover">
          + New
        </button>
      </div>

      {/* Session List */}
      <div className="flex-1 overflow-y-auto p-2">
        {sessions.length === 0 ? (
          <div className="px-2 py-8 text-center text-xs text-zinc-500">
            No active sessions.
            <br />
            Click + New to start.
          </div>
        ) : (
          <ul className="space-y-1">
            {sessions.map((session) => (
              <li
                key={session.id}
                onClick={() => setActiveSession(session.id)}
                className={`group cursor-pointer rounded-md px-3 py-2 text-sm transition-colors ${
                  activeSessionId === session.id
                    ? "bg-surface-3 text-white"
                    : "text-zinc-400 hover:bg-surface-2 hover:text-zinc-200"
                }`}
              >
                <div className="flex items-center gap-2">
                  <span
                    className={`h-2 w-2 rounded-full ${statusColor[session.status]}`}
                  />
                  <span className="truncate font-medium">
                    {session.branch}
                  </span>
                </div>
                <div className="ml-4 mt-0.5 flex items-center justify-between text-xs text-zinc-500">
                  <span>{session.agent}</span>
                  <button
                    onClick={(e) => {
                      e.stopPropagation();
                      stopSession(session.id);
                    }}
                    className="hidden text-red-400 hover:text-red-300 group-hover:block"
                  >
                    Stop
                  </button>
                </div>
              </li>
            ))}
          </ul>
        )}
      </div>
    </aside>
  );
}
