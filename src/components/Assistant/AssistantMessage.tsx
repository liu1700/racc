import Markdown from "react-markdown";

interface Props {
  role: "user" | "assistant" | "tool_call" | "tool_result";
  content: string;
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
          </div>
        )}
      </div>
    </div>
  );
}
