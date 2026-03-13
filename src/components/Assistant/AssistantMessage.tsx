import Markdown from "react-markdown";
import { useFileViewerStore } from "../../stores/fileViewerStore";
import { useSessionStore } from "../../stores/sessionStore";

interface Props {
  role: "user" | "assistant" | "tool_call" | "tool_result";
  content: string;
}

function OpenFileButton({ content }: { content: string }) {
  // Match pattern: "filename.ext · Lines X-Y (Z total)" or just "filename.ext"
  const fileMatch = content.match(/^(\S+\.\w+)\s*·/m);
  if (!fileMatch) return null;

  const filePath = fileMatch[1];

  const handleClick = () => {
    const activeSession = useSessionStore.getState().getActiveSession();
    if (!activeSession) return;

    // Try to extract line number from "Lines X-Y" pattern
    const lineMatch = content.match(/Lines?\s+(\d+)/i);
    const scrollToLine = lineMatch ? parseInt(lineMatch[1], 10) : undefined;

    useFileViewerStore.getState().openFile({
      sessionId: activeSession.session.id,
      repoId: activeSession.repo.id,
      filePath,
      scrollToLine,
    });
  };

  return (
    <button
      onClick={handleClick}
      className="mt-1 text-xs text-accent hover:text-accent-hover"
    >
      [ Open Full File ↗ ]
    </button>
  );
}

export function AssistantMessage({ role, content }: Props) {
  if (role === "tool_call" || role === "tool_result") return null;

  const isUser = role === "user";

  return (
    <div className={`flex ${isUser ? "justify-end" : "justify-start"}`}>
      <div
        className={`max-w-[90%] rounded-lg px-3 py-2 text-xs ${
          isUser
            ? "bg-accent/20 text-zinc-200"
            : "bg-surface-2 text-zinc-300"
        }`}
      >
        {isUser ? (
          <p className="whitespace-pre-wrap">{content}</p>
        ) : (
          <div className="prose prose-invert prose-sm max-w-none [&_pre]:bg-surface-0 [&_pre]:p-2 [&_pre]:rounded [&_code]:text-[11px] [&_p]:my-1 [&_h2]:text-xs [&_h2]:mt-2 [&_h2]:mb-1 [&_h3]:text-xs [&_h3]:mt-2 [&_h3]:mb-1 [&_ul]:my-1 [&_li]:my-0">
            <Markdown>{content}</Markdown>
            <OpenFileButton content={content} />
          </div>
        )}
      </div>
    </div>
  );
}
