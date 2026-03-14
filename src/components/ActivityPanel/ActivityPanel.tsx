import { useEffect, useRef } from "react";
import { useSessionStore } from "../../stores/sessionStore";
import type { SessionActivity } from "../../types/session";

/** Map activity action to a status dot color class. */
function activityDotClass(activity: SessionActivity): string {
  switch (activity.action) {
    case "Waiting for approval":
      return "bg-status-waiting";
    case "Idle":
      return "bg-status-running/50";
    case "Completed":
      return activity.detail === "exit 0" ? "bg-status-completed" : "bg-status-error";
    default:
      return "bg-status-running";
  }
}

/** Whether this activity's dot should pulse. */
function shouldPulse(action: string): boolean {
  return action !== "Idle" && action !== "Completed" && action !== "Waiting for approval";
}

/** Look up the session's branch from the repos list. */
function useSessionBranch(sessionId: number): { agent: string; branch: string } {
  const repos = useSessionStore((s) => s.repos);
  for (const rws of repos) {
    const session = rws.sessions.find((s) => s.id === sessionId);
    if (session) {
      return { agent: session.agent, branch: session.branch ?? "main" };
    }
  }
  return { agent: "agent", branch: "main" };
}

function ActivityBar({
  activity,
  isActive,
  onSelect,
}: {
  activity: SessionActivity;
  isActive: boolean;
  onSelect: () => void;
}) {
  const { agent, branch } = useSessionBranch(activity.sessionId);
  const isCompleted = activity.action === "Completed";

  return (
    <button
      type="button"
      onClick={onSelect}
      className={`flex w-full cursor-pointer items-center justify-between px-4 py-1 text-xs transition-colors duration-150 ${
        isActive ? "border-l-2 border-accent bg-surface-2" : "border-l-2 border-transparent hover:bg-surface-3"
      } ${isCompleted ? "animate-fade-out" : ""}`}
      style={{ height: "28px" }}
    >
      {/* Left: status dot + agent + branch */}
      <span className="flex items-center gap-2 overflow-hidden">
        <span
          className={`h-1.5 w-1.5 flex-shrink-0 rounded-full ${activityDotClass(activity)} ${
            shouldPulse(activity.action) ? "animate-status-pulse" : ""
          }`}
        />
        <span className="truncate text-zinc-400">
          {agent} <span className="text-zinc-500">({branch})</span>
        </span>
      </span>

      {/* Right: action + detail */}
      <span className="ml-4 max-w-[50%] truncate text-right text-zinc-400">
        {activity.action}
        {activity.detail && (
          <span className="ml-1 text-zinc-300">{activity.detail}</span>
        )}
      </span>
    </button>
  );
}

export function ActivityPanel() {
  const activities = useSessionStore((s) => s.sessionActivities);
  const panelOpen = useSessionStore((s) => s.activityPanelOpen);
  const dismissPanel = useSessionStore((s) => s.dismissActivityPanel);
  const setActiveSession = useSessionStore((s) => s.setActiveSession);
  const activeSessionId = useSessionStore((s) => s.activeSessionId);
  const removeSessionActivity = useSessionStore((s) => s.removeSessionActivity);

  // Track fade-out timeouts for cleanup
  const fadeTimers = useRef<Map<number, ReturnType<typeof setTimeout>>>(new Map());

  const activityList = Object.values(activities);

  // Clean up all fade timers on unmount only
  useEffect(() => {
    return () => {
      for (const [, timer] of fadeTimers.current) {
        clearTimeout(timer);
      }
    };
  }, []);

  // Set up fade-out timers for completed sessions
  // The .has() guard prevents duplicate timers even though activityList is a new ref each render
  useEffect(() => {
    for (const activity of activityList) {
      if (activity.action === "Completed" && !fadeTimers.current.has(activity.sessionId)) {
        const timer = setTimeout(() => {
          removeSessionActivity(activity.sessionId);
          fadeTimers.current.delete(activity.sessionId);
        }, 5000);
        fadeTimers.current.set(activity.sessionId, timer);
      }
    }
  }, [activityList, removeSessionActivity]);

  // Nothing to show
  if (activityList.length === 0 && !panelOpen) return null;

  return (
    <div
      className={`border-t border-surface-3 bg-surface-1 transition-all duration-150 ${
        panelOpen ? "max-h-40 opacity-100" : "max-h-0 opacity-0 overflow-hidden"
      }`}
    >
      {/* Header */}
      <div className="flex items-center justify-between px-4 py-0.5">
        <span className="text-xs font-medium uppercase tracking-wider text-zinc-500">
          Activity
        </span>
        <button
          type="button"
          onClick={dismissPanel}
          className="text-zinc-500 hover:text-zinc-300 transition-colors text-xs px-1"
          title="Collapse activity panel"
        >
          ▾
        </button>
      </div>

      {/* Session bars */}
      <div className="max-h-[140px] overflow-y-auto">
        {activityList.map((activity) => (
          <ActivityBar
            key={activity.sessionId}
            activity={activity}
            isActive={activity.sessionId === activeSessionId}
            onSelect={() => setActiveSession(activity.sessionId)}
          />
        ))}
      </div>
    </div>
  );
}
