import { useState, useEffect, useRef, useCallback } from "react";
import { useServerStore } from "../../stores/serverStore";
import { createSetupAgent } from "../../services/setupAgent";
import { StaticGuide } from "./StaticGuide";
import type { Agent } from "@mariozechner/pi-agent-core";

interface SetupWizardProps {
  serverId: string;
  onDone: () => void;
}

interface ChatMessage {
  role: "assistant" | "user" | "system";
  content: string;
}

export function SetupWizard({ serverId, onDone }: SetupWizardProps) {
  const servers = useServerStore((s) => s.servers);
  const server = servers.find((s) => s.id === serverId);

  const hasAiKey = !!(server?.ai_provider && server?.ai_api_key);

  if (!hasAiKey) {
    return (
      <div
        className="fixed inset-0 z-50 flex items-center justify-center bg-black/60"
        onKeyDown={(e) => e.key === "Escape" && onDone()}
      >
        <div className="w-[420px] max-h-[90vh] overflow-y-auto rounded-lg border border-surface-3 bg-surface-1 p-5 shadow-2xl">
          <StaticGuide serverId={serverId} onDone={onDone} />
        </div>
      </div>
    );
  }

  return <AiSetupChat serverId={serverId} server={server} onDone={onDone} />;
}

// ---------------------------------------------------------------------------
// AI Chat sub-component
// ---------------------------------------------------------------------------

interface AiSetupChatProps {
  serverId: string;
  server: { ai_provider: string | null; ai_api_key: string | null; name: string };
  onDone: () => void;
}

function AiSetupChat({ serverId, server, onDone }: AiSetupChatProps) {
  const [messages, setMessages] = useState<ChatMessage[]>([]);
  const [input, setInput] = useState("");
  const [loading, setLoading] = useState(false);
  const [confirmCmd, setConfirmCmd] = useState<{
    command: string;
    resolve: (ok: boolean) => void;
  } | null>(null);

  const agentRef = useRef<Agent | null>(null);
  const scrollRef = useRef<HTMLDivElement>(null);
  const pendingTextRef = useRef("");

  // Auto-scroll to bottom when messages change
  useEffect(() => {
    scrollRef.current?.scrollTo({
      top: scrollRef.current.scrollHeight,
      behavior: "smooth",
    });
  }, [messages]);

  // Flush streaming text into the latest assistant message
  const flushPending = useCallback(() => {
    const text = pendingTextRef.current;
    if (!text) return;
    pendingTextRef.current = "";
    setMessages((prev) => {
      const last = prev[prev.length - 1];
      if (last?.role === "assistant") {
        return [...prev.slice(0, -1), { ...last, content: last.content + text }];
      }
      return [...prev, { role: "assistant", content: text }];
    });
  }, []);

  // Initialize agent
  useEffect(() => {
    let cancelled = false;

    (async () => {
      try {
        const agent = await createSetupAgent({
          serverId,
          provider: server.ai_provider ?? "anthropic",
          apiKey: server.ai_api_key ?? "",
          onMessage: (delta) => {
            pendingTextRef.current += delta;
          },
          onCommandRun: (cmd, output) => {
            // Flush any pending assistant text first
            setMessages((prev) => {
              const pending = pendingTextRef.current;
              pendingTextRef.current = "";
              const msgs = [...prev];
              if (pending) {
                const last = msgs[msgs.length - 1];
                if (last?.role === "assistant") {
                  msgs[msgs.length - 1] = { ...last, content: last.content + pending };
                } else {
                  msgs.push({ role: "assistant", content: pending });
                }
              }
              msgs.push({
                role: "system",
                content: `$ ${cmd}\n${output}`,
              });
              return msgs;
            });
          },
          onConfirmNeeded: (cmd) => {
            return new Promise<boolean>((resolve) => {
              setConfirmCmd({ command: cmd, resolve });
            });
          },
        });

        if (cancelled) return;
        agentRef.current = agent;

        // Start the agent with initial prompt
        setLoading(true);
        setMessages([{ role: "assistant", content: "" }]);
        await agent.prompt("Check this server and help me set it up");

        // Flush remaining streamed text
        if (pendingTextRef.current) {
          const remaining = pendingTextRef.current;
          pendingTextRef.current = "";
          setMessages((prev) => {
            const last = prev[prev.length - 1];
            if (last?.role === "assistant") {
              return [...prev.slice(0, -1), { ...last, content: last.content + remaining }];
            }
            return [...prev, { role: "assistant", content: remaining }];
          });
        }
      } catch (err) {
        if (!cancelled) {
          setMessages((prev) => [
            ...prev,
            { role: "system", content: `Error: ${err instanceof Error ? err.message : String(err)}` },
          ]);
        }
      } finally {
        if (!cancelled) setLoading(false);
      }
    })();

    // Periodic flush for streaming text
    const interval = setInterval(flushPending, 150);

    return () => {
      cancelled = true;
      clearInterval(interval);
    };
  }, [serverId, server.ai_provider, server.ai_api_key, flushPending]);

  const handleSend = async () => {
    const text = input.trim();
    if (!text || !agentRef.current || loading) return;

    setInput("");
    setMessages((prev) => [...prev, { role: "user", content: text }]);
    setLoading(true);
    setMessages((prev) => [...prev, { role: "assistant", content: "" }]);

    try {
      await agentRef.current.prompt(text);
      // Flush remaining
      if (pendingTextRef.current) {
        const remaining = pendingTextRef.current;
        pendingTextRef.current = "";
        setMessages((prev) => {
          const last = prev[prev.length - 1];
          if (last?.role === "assistant") {
            return [...prev.slice(0, -1), { ...last, content: last.content + remaining }];
          }
          return [...prev, { role: "assistant", content: remaining }];
        });
      }
    } catch (err) {
      setMessages((prev) => [
        ...prev,
        { role: "system", content: `Error: ${err instanceof Error ? err.message : String(err)}` },
      ]);
    } finally {
      setLoading(false);
    }
  };

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/60"
      onKeyDown={(e) => e.key === "Escape" && onDone()}
    >
      <div className="flex h-[80vh] w-[520px] flex-col rounded-lg border border-surface-3 bg-surface-1 shadow-2xl">
        {/* Header */}
        <div className="flex items-center justify-between border-b border-surface-3 px-4 py-3">
          <h2 className="text-sm font-semibold text-zinc-200">
            Setup Assistant &mdash; {server.name}
          </h2>
          <button
            onClick={onDone}
            className="text-xs text-zinc-500 hover:text-zinc-300"
          >
            Close
          </button>
        </div>

        {/* Messages */}
        <div ref={scrollRef} className="flex-1 overflow-y-auto px-4 py-3 space-y-3">
          {messages.map((msg, i) => (
            <div key={i}>
              {msg.role === "user" && (
                <div className="flex justify-end">
                  <div className="max-w-[80%] rounded-lg bg-accent/20 px-3 py-2 text-xs text-zinc-200">
                    {msg.content}
                  </div>
                </div>
              )}
              {msg.role === "assistant" && msg.content && (
                <div className="max-w-[90%] text-xs leading-relaxed text-zinc-300 whitespace-pre-wrap">
                  {msg.content}
                </div>
              )}
              {msg.role === "system" && (
                <div className="rounded border border-surface-3 bg-surface-0 px-3 py-2">
                  <pre className="overflow-x-auto text-[11px] leading-relaxed text-zinc-400 whitespace-pre-wrap">
                    {msg.content}
                  </pre>
                </div>
              )}
            </div>
          ))}
          {loading && (
            <span className="text-[11px] text-zinc-500 animate-pulse">
              Thinking...
            </span>
          )}
        </div>

        {/* Confirmation dialog */}
        {confirmCmd && (
          <div className="border-t border-surface-3 bg-surface-0 px-4 py-3">
            <p className="mb-2 text-xs text-zinc-300">
              The assistant wants to run:
            </p>
            <code className="mb-2 block overflow-x-auto rounded border border-surface-3 bg-surface-1 px-2 py-1.5 text-[11px] text-zinc-300">
              {confirmCmd.command}
            </code>
            <div className="flex gap-2">
              <button
                onClick={() => {
                  confirmCmd.resolve(true);
                  setConfirmCmd(null);
                }}
                className="rounded bg-accent px-3 py-1 text-xs font-medium text-white hover:opacity-90"
              >
                Allow
              </button>
              <button
                onClick={() => {
                  confirmCmd.resolve(false);
                  setConfirmCmd(null);
                }}
                className="rounded border border-surface-3 px-3 py-1 text-xs text-zinc-400 hover:bg-surface-2 hover:text-zinc-200"
              >
                Deny
              </button>
            </div>
          </div>
        )}

        {/* Input */}
        <div className="border-t border-surface-3 px-4 py-3">
          <div className="flex gap-2">
            <input
              type="text"
              value={input}
              onChange={(e) => setInput(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter" && !e.shiftKey) {
                  e.preventDefault();
                  handleSend();
                }
              }}
              placeholder={loading ? "Waiting for response..." : "Type a message..."}
              disabled={loading}
              className="flex-1 rounded border border-surface-3 bg-surface-0 px-2 py-1.5 text-xs text-zinc-200 placeholder-zinc-600 focus:border-accent focus:outline-none disabled:opacity-50"
            />
            <button
              onClick={handleSend}
              disabled={loading || !input.trim()}
              className="rounded bg-accent px-3 py-1.5 text-xs font-medium text-white hover:opacity-90 disabled:opacity-50"
            >
              Send
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}
