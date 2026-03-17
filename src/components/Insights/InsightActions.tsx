import { transport } from "../../services/transport";
import { useSessionStore } from "../../stores/sessionStore";
import { useFileViewerStore } from "../../stores/fileViewerStore";

interface InsightActionsProps {
  insightType: string;
  detail: Record<string, unknown>;
  onApply: () => void;
  onDismiss: () => void;
}

export function InsightActions({ insightType, detail, onApply, onDismiss }: InsightActionsProps) {
  const setActiveSession = useSessionStore((s) => s.setActiveSession);

  const handleAddToClaudeMd = async () => {
    const suggested = (detail.suggestedEntry as string) || (detail.matches as Array<{ text: string }>)?.[0]?.text;
    if (!suggested) return;

    const activeData = useSessionStore.getState().getActiveSession();
    const repoPath = activeData?.repo.path;
    if (!repoPath) return;

    try {
      await transport.call("append_to_file", {
        path: `${repoPath}/CLAUDE.md`,
        content: `\n${suggested}`,
      });
      onApply();
    } catch (e) {
      console.error("Failed to append to CLAUDE.md:", e);
    }
  };

  const handleCopyRule = async () => {
    const perm = detail.permissionType as string;
    if (perm) {
      await navigator.clipboard.writeText(`Allow: ${perm}`);
      onApply();
    }
  };

  const handleSwitchToSession = (sessionId: number) => {
    setActiveSession(sessionId);
  };

  switch (insightType) {
    case "repeated_prompt":
    case "startup_pattern":
      return (
        <div className="flex gap-2">
          <button
            onClick={handleAddToClaudeMd}
            className="rounded-md bg-status-completed/20 px-3 py-1.5 text-[11px] font-medium text-status-completed hover:bg-status-completed/30"
          >
            Add to CLAUDE.md
          </button>
          <button
            onClick={onDismiss}
            className="rounded-md bg-surface-2 px-3 py-1.5 text-[11px] text-text-tertiary hover:bg-surface-3"
          >
            Dismiss
          </button>
        </div>
      );

    case "repeated_permission":
      return (
        <div className="flex gap-2">
          <button
            onClick={handleCopyRule}
            className="rounded-md bg-surface-2 px-3 py-1.5 text-[11px] font-medium text-text-primary hover:bg-surface-3"
          >
            Copy allowlist rule
          </button>
          <button
            onClick={onDismiss}
            className="rounded-md bg-surface-2 px-3 py-1.5 text-[11px] text-text-tertiary hover:bg-surface-3"
          >
            Dismiss
          </button>
        </div>
      );

    case "cost_anomaly":
      return (
        <div className="flex gap-2">
          <button
            onClick={() => handleSwitchToSession(detail.sessionId as number)}
            className="rounded-md bg-accent/20 px-3 py-1.5 text-[11px] font-medium text-accent hover:bg-accent/30"
          >
            Switch to session
          </button>
          <button
            onClick={onDismiss}
            className="rounded-md bg-surface-2 px-3 py-1.5 text-[11px] text-text-tertiary hover:bg-surface-3"
          >
            Dismiss
          </button>
        </div>
      );

    case "file_conflict": {
      const sessions = (detail.sessions as Array<{ sessionId: number }>) || [];
      const filePath = detail.filePath as string;
      const handleViewFile = () => {
        if (filePath && sessions.length > 0) {
          useFileViewerStore.getState().openFile({
            sessionId: sessions[0].sessionId,
            filePath,
          });
        }
      };
      return (
        <div className="flex flex-wrap gap-2">
          <button
            onClick={handleViewFile}
            className="rounded-md bg-status-error/20 px-3 py-1.5 text-[11px] font-medium text-status-error hover:bg-status-error/30"
          >
            View File
          </button>
          {sessions.map((s) => (
            <button
              key={s.sessionId}
              onClick={() => handleSwitchToSession(s.sessionId)}
              className="rounded-md bg-accent/20 px-3 py-1.5 text-[11px] font-medium text-accent hover:bg-accent/30"
            >
              Session {s.sessionId}
            </button>
          ))}
          <button
            onClick={onDismiss}
            className="rounded-md bg-surface-2 px-3 py-1.5 text-[11px] text-text-tertiary hover:bg-surface-3"
          >
            Dismiss
          </button>
        </div>
      );
    }

    case "similar_sessions": {
      const sessionA = (detail.sessionA as { id: number })?.id;
      const sessionB = (detail.sessionB as { id: number })?.id;
      return (
        <div className="flex gap-2">
          {sessionA && (
            <button
              onClick={() => handleSwitchToSession(sessionA)}
              className="rounded-md bg-accent/20 px-3 py-1.5 text-[11px] font-medium text-accent hover:bg-accent/30"
            >
              Session {sessionA}
            </button>
          )}
          {sessionB && (
            <button
              onClick={() => handleSwitchToSession(sessionB)}
              className="rounded-md bg-accent/20 px-3 py-1.5 text-[11px] font-medium text-accent hover:bg-accent/30"
            >
              Session {sessionB}
            </button>
          )}
          <button
            onClick={onDismiss}
            className="rounded-md bg-surface-2 px-3 py-1.5 text-[11px] text-text-tertiary hover:bg-surface-3"
          >
            Dismiss
          </button>
        </div>
      );
    }

    default:
      return (
        <button
          onClick={onDismiss}
          className="rounded-md bg-surface-2 px-3 py-1.5 text-[11px] text-text-tertiary hover:bg-surface-3"
        >
          Dismiss
        </button>
      );
  }
}
