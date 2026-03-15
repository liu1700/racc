import { useState, useEffect, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";

interface StaticGuideProps {
  serverId: string;
  onDone: () => void;
}

interface ToolStatus {
  name: string;
  command: string;
  installLabel: string;
  installCommand: string;
  detected: boolean | null; // null = not checked yet
}

const INITIAL_TOOLS: ToolStatus[] = [
  {
    name: "git",
    command: "which git",
    installLabel: "Install git",
    installCommand: "sudo apt install -y git",
    detected: null,
  },
  {
    name: "tmux",
    command: "which tmux",
    installLabel: "Install tmux",
    installCommand: "sudo apt install -y tmux",
    detected: null,
  },
  {
    name: "Claude Code",
    command: "which claude",
    installLabel: "Install Claude Code",
    installCommand: "npm install -g @anthropic-ai/claude-code",
    detected: null,
  },
  {
    name: "Codex",
    command: "which codex",
    installLabel: "Install Codex (optional)",
    installCommand: "npm install -g @openai/codex",
    detected: null,
  },
];

export function StaticGuide({ serverId, onDone }: StaticGuideProps) {
  const [tools, setTools] = useState<ToolStatus[]>(INITIAL_TOOLS);
  const [checking, setChecking] = useState(false);
  const [copied, setCopied] = useState<string | null>(null);

  const runDetection = useCallback(async () => {
    setChecking(true);
    const updated = await Promise.all(
      INITIAL_TOOLS.map(async (tool) => {
        try {
          const result = await invoke<{ exit_code: number }>(
            "execute_remote_command",
            { serverId, command: tool.command },
          );
          return { ...tool, detected: result.exit_code === 0 };
        } catch {
          return { ...tool, detected: false };
        }
      }),
    );
    setTools(updated);
    setChecking(false);
  }, [serverId]);

  useEffect(() => {
    runDetection();
  }, [runDetection]);

  const copyToClipboard = async (text: string) => {
    try {
      await navigator.clipboard.writeText(text);
      setCopied(text);
      setTimeout(() => setCopied(null), 1500);
    } catch {
      // clipboard not available
    }
  };

  const missingTools = tools.filter((t) => t.detected === false);
  const allReady = missingTools.length === 0 && tools.every((t) => t.detected !== null);

  return (
    <div className="space-y-4">
      <div>
        <h3 className="text-sm font-semibold text-zinc-200">Server Setup</h3>
        <p className="mt-1 text-xs text-zinc-400">
          {checking
            ? "Checking installed tools..."
            : allReady
              ? "All required tools are installed."
              : "Please run these commands on your server."}
        </p>
      </div>

      {/* Tool checklist */}
      <div className="space-y-3">
        {tools.map((tool) => (
          <div key={tool.name} className="space-y-1">
            <div className="flex items-center gap-2">
              {tool.detected === null ? (
                <span className="h-3.5 w-3.5 flex-shrink-0 rounded-sm border border-zinc-600" />
              ) : tool.detected ? (
                <span className="flex h-3.5 w-3.5 flex-shrink-0 items-center justify-center rounded-sm bg-green-600 text-[9px] text-white">
                  &#10003;
                </span>
              ) : (
                <span className="flex h-3.5 w-3.5 flex-shrink-0 items-center justify-center rounded-sm bg-zinc-600 text-[9px] text-zinc-300">
                  &mdash;
                </span>
              )}
              <span className="text-xs text-zinc-300">
                {tool.detected ? tool.name : tool.installLabel}
              </span>
            </div>

            {/* Show install command only for missing tools */}
            {tool.detected === false && (
              <div className="ml-5 flex items-stretch gap-1">
                <code className="flex-1 overflow-x-auto rounded border border-surface-3 bg-surface-0 px-2 py-1.5 text-[11px] leading-relaxed text-zinc-300">
                  {tool.installCommand}
                </code>
                <button
                  onClick={() => copyToClipboard(tool.installCommand)}
                  className="flex-shrink-0 rounded border border-surface-3 bg-surface-0 px-2 text-[10px] text-zinc-400 transition-colors hover:bg-surface-2 hover:text-zinc-200"
                >
                  {copied === tool.installCommand ? "Copied" : "Copy"}
                </button>
              </div>
            )}
          </div>
        ))}
      </div>

      {/* Tip */}
      <p className="rounded bg-surface-0 px-3 py-2 text-[11px] text-zinc-500">
        Tip: Set up an AI API key in server settings for intelligent setup
        assistance.
      </p>

      {/* Actions */}
      <div className="flex justify-end gap-2">
        <button
          onClick={runDetection}
          disabled={checking}
          className="rounded border border-surface-3 px-3 py-1.5 text-xs text-zinc-400 transition-colors hover:bg-surface-2 hover:text-zinc-200 disabled:opacity-50"
        >
          {checking ? "Checking..." : "Re-check"}
        </button>
        <button
          onClick={onDone}
          className="rounded bg-accent px-3 py-1.5 text-xs font-medium text-white hover:opacity-90"
        >
          Done
        </button>
      </div>
    </div>
  );
}
