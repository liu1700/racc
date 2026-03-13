import { useState, useRef, useEffect } from "react";
import { useAssistantStore } from "../../stores/assistantStore";
import { useShallow } from "zustand/react/shallow";
import { AssistantMessage } from "./AssistantMessage";
import Markdown from "react-markdown";

export function AssistantChat() {
  const { messages, isStreaming, streamingText, error, sendMessage, clearError } = useAssistantStore(
    useShallow((s) => ({
      messages: s.messages,
      isStreaming: s.isStreaming,
      streamingText: s.streamingText,
      error: s.error,
      sendMessage: s.sendMessage,
      clearError: s.clearError,
    }))
  );
  const [input, setInput] = useState("");
  const messagesEndRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    messagesEndRef.current?.scrollIntoView({ behavior: "smooth" });
  }, [messages, streamingText]);

  const handleSend = () => {
    const trimmed = input.trim();
    if (!trimmed || isStreaming) return;
    setInput("");
    sendMessage(trimmed);
  };

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === "Enter" && !e.shiftKey) {
      e.preventDefault();
      handleSend();
    }
  };

  const quickActions = [
    { label: "Summarize Diff", prompt: "Summarize what my agents have changed. Show me a risk-prioritized overview." },
    { label: "Costs", prompt: "What are the current costs across all my sessions?" },
  ];

  return (
    <div className="flex flex-1 flex-col overflow-hidden">
      {/* Message list */}
      <div className="flex-1 overflow-y-auto p-3 space-y-3">
        {messages.length === 0 && !isStreaming && (
          <div className="flex items-center justify-center py-8 text-xs text-zinc-600">
            Ask me about your agents' work.
          </div>
        )}

        {messages.map((msg) => (
          <AssistantMessage key={msg.id} role={msg.role} content={msg.content} />
        ))}

        {/* Streaming message */}
        {isStreaming && streamingText && (
          <div className="flex justify-start">
            <div className="max-w-[90%] rounded-lg bg-surface-2 px-3 py-2 text-xs text-zinc-300">
              <div className="prose prose-invert prose-sm max-w-none [&_pre]:bg-surface-0 [&_pre]:p-2 [&_pre]:rounded [&_code]:text-[11px] [&_p]:my-1 [&_h2]:text-xs [&_h2]:mt-2 [&_h2]:mb-1 [&_h3]:text-xs [&_h3]:mt-2 [&_h3]:mb-1 [&_ul]:my-1 [&_li]:my-0">
                <Markdown>{streamingText}</Markdown>
              </div>
            </div>
          </div>
        )}

        {isStreaming && !streamingText && (
          <div className="flex justify-start">
            <div className="rounded-lg bg-surface-2 px-3 py-2 text-xs text-zinc-500">
              Thinking...
            </div>
          </div>
        )}

        {error && (
          <div className="flex items-start gap-2 rounded-lg bg-red-500/10 border border-red-500/20 px-3 py-2 text-xs text-red-400">
            <span className="flex-1">{error}</span>
            <button onClick={clearError} className="shrink-0 text-red-500 hover:text-red-300">&times;</button>
          </div>
        )}

        <div ref={messagesEndRef} />
      </div>

      {/* Quick actions + Input */}
      <div className="border-t border-surface-3 p-2">
        <div className="mb-2 flex gap-1">
          {quickActions.map((action) => (
            <button
              key={action.label}
              onClick={() => {
                if (!isStreaming) sendMessage(action.prompt);
              }}
              disabled={isStreaming}
              className="rounded bg-surface-2 px-2 py-1 text-[10px] text-zinc-400 transition-colors duration-150 hover:bg-surface-3 hover:text-zinc-300 disabled:opacity-50"
            >
              {action.label}
            </button>
          ))}
        </div>
        <div className="flex gap-2">
          <input
            type="text"
            value={input}
            onChange={(e) => setInput(e.target.value)}
            onKeyDown={handleKeyDown}
            placeholder="Ask about your agents..."
            disabled={isStreaming}
            className="flex-1 rounded border border-surface-3 bg-surface-0 px-2 py-1.5 text-xs text-zinc-300 placeholder-zinc-600 outline-none focus:border-accent disabled:opacity-50"
          />
          <button
            onClick={handleSend}
            disabled={isStreaming || !input.trim()}
            className="rounded bg-accent px-3 py-1.5 text-xs font-medium text-white transition-colors duration-150 hover:bg-accent-hover disabled:opacity-50"
          >
            Send
          </button>
        </div>
      </div>
    </div>
  );
}
