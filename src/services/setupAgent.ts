/**
 * Setup Agent Service
 *
 * Uses @mariozechner/pi-agent-core and @mariozechner/pi-ai to run an AI agent
 * that helps users configure remote servers for running AI coding agents.
 */

import { invoke } from "@tauri-apps/api/core";
import { Agent, type AgentEvent, type AgentTool, type AgentToolResult } from "@mariozechner/pi-agent-core";
import { Type, type Static } from "@mariozechner/pi-ai";
import { getModel, getModels } from "@mariozechner/pi-ai";
import type { KnownProvider, Model, Api } from "@mariozechner/pi-ai";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

export interface SetupAgentOptions {
  serverId: string;
  provider: string; // "anthropic" | "openai" | "openrouter"
  apiKey: string;
  onMessage: (text: string) => void;
  onCommandRun: (command: string, output: string) => void;
  onConfirmNeeded: (command: string) => Promise<boolean>;
}

interface CommandOutput {
  stdout: string;
  stderr: string;
  exit_code: number;
}

interface Server {
  id: string;
  name: string;
  host: string;
  port: number;
  username: string;
  auth_method: string;
  key_path: string | null;
  ssh_config_host: string | null;
  setup_status: string;
  setup_details: string | null;
  ai_provider: string | null;
  ai_api_key: string | null;
  created_at: string;
  updated_at: string;
}

// ---------------------------------------------------------------------------
// System prompt
// ---------------------------------------------------------------------------

const SYSTEM_PROMPT = `You are a server setup assistant for Racc, an Agentic IDE. Your job is to help configure a remote Linux/macOS server so it can run AI coding agents (Claude Code, Aider, Codex) via SSH + tmux.

## Your approach

1. **Gather information first.** Use get_server_info to learn the server config, then run diagnostic commands:
   - Check OS and version: \`uname -a\`, \`cat /etc/os-release\`
   - Check available package manager: \`which apt yum dnf brew pacman 2>/dev/null\`
   - Check installed tools: \`which git tmux node python3 2>/dev/null\`

2. **PRIORITIZE login/authentication setup.** Before installing tools, ensure:
   - The AI coding agent CLI can authenticate (API keys, OAuth tokens)
   - Environment variables or config files are properly set

3. **Ensure required tools are installed:**
   - \`git\` — must be installed and able to access repositories
   - \`tmux\` — required for persistent sessions
   - \`node\` / \`npm\` — needed for Claude Code and some agents
   - The chosen AI agent CLI itself (e.g., \`claude\`, \`aider\`, \`codex\`)

4. **Adapt to the server's OS and package manager.** Use apt on Debian/Ubuntu, yum/dnf on RHEL/CentOS/Fedora, brew on macOS, etc.

5. **Always ask for confirmation** before installing packages or modifying system configuration. Set requires_confirmation to true for any command that installs, removes, or modifies files.

6. **Be concise.** Report what you find and what you plan to do. Don't over-explain basics.

## Safety rules

- Never run \`rm -rf /\` or similar destructive commands
- Never modify SSH config in ways that could lock out the user
- Always use requires_confirmation=true for: package installs, config file writes, service restarts
- For read-only / diagnostic commands, requires_confirmation can be false
`;

// ---------------------------------------------------------------------------
// Tool definitions
// ---------------------------------------------------------------------------

const RunRemoteCommandParams = Type.Object({
  command: Type.String({ description: "The shell command to execute on the remote server" }),
  requires_confirmation: Type.Boolean({
    description:
      "Whether this command requires user confirmation before executing. Set to true for commands that install packages, modify config, or make persistent changes. Set to false for read-only diagnostic commands.",
    default: false,
  }),
});

const GetServerInfoParams = Type.Object({});

function createRunRemoteCommandTool(options: SetupAgentOptions): AgentTool<typeof RunRemoteCommandParams> {
  return {
    name: "run_remote_command",
    label: "Run Remote Command",
    description:
      "Execute a shell command on the remote server via SSH. Use requires_confirmation=true for commands that modify the system.",
    parameters: RunRemoteCommandParams,
    execute: async (
      _toolCallId: string,
      params: Static<typeof RunRemoteCommandParams>,
    ): Promise<AgentToolResult<CommandOutput | { error: string }>> => {
      const { command, requires_confirmation } = params;

      // If confirmation is needed, ask the user
      if (requires_confirmation) {
        const confirmed = await options.onConfirmNeeded(command);
        if (!confirmed) {
          return {
            content: [{ type: "text", text: "Command was rejected by the user." }],
            details: { error: "User rejected command" },
          };
        }
      }

      try {
        const result = await invoke<CommandOutput>("execute_remote_command", {
          serverId: options.serverId,
          command,
        });

        const outputParts: string[] = [];
        if (result.stdout) outputParts.push(`stdout:\n${result.stdout}`);
        if (result.stderr) outputParts.push(`stderr:\n${result.stderr}`);
        outputParts.push(`exit_code: ${result.exit_code}`);
        const outputText = outputParts.join("\n\n");

        options.onCommandRun(command, outputText);

        return {
          content: [{ type: "text", text: outputText }],
          details: result,
        };
      } catch (err) {
        const errorMessage = err instanceof Error ? err.message : String(err);
        options.onCommandRun(command, `Error: ${errorMessage}`);
        return {
          content: [{ type: "text", text: `Failed to execute command: ${errorMessage}` }],
          details: { error: errorMessage },
        };
      }
    },
  };
}

function createGetServerInfoTool(options: SetupAgentOptions): AgentTool<typeof GetServerInfoParams> {
  return {
    name: "get_server_info",
    label: "Get Server Info",
    description: "Get the configuration details of the current remote server (host, port, username, setup status, etc.)",
    parameters: GetServerInfoParams,
    execute: async (
      _toolCallId: string,
      _params: Static<typeof GetServerInfoParams>,
    ): Promise<AgentToolResult<Server | null>> => {
      try {
        const servers = await invoke<Server[]>("list_servers");
        const server = servers.find((s) => s.id === options.serverId) ?? null;

        if (!server) {
          return {
            content: [{ type: "text", text: `Server with id "${options.serverId}" not found.` }],
            details: null,
          };
        }

        // Redact the API key for safety
        const safeServer = { ...server, ai_api_key: server.ai_api_key ? "***" : null };
        return {
          content: [{ type: "text", text: JSON.stringify(safeServer, null, 2) }],
          details: server,
        };
      } catch (err) {
        const errorMessage = err instanceof Error ? err.message : String(err);
        return {
          content: [{ type: "text", text: `Failed to get server info: ${errorMessage}` }],
          details: null,
        };
      }
    },
  };
}

// ---------------------------------------------------------------------------
// Model resolution
// ---------------------------------------------------------------------------

/** Default model IDs to try for each provider */
const DEFAULT_MODELS: Record<string, string> = {
  anthropic: "claude-sonnet-4-20250514",
  openai: "gpt-4.1",
  openrouter: "anthropic/claude-sonnet-4",
};

function resolveModel(provider: string): Model<Api> | null {
  const knownProvider = provider as KnownProvider;

  // Try the default model for this provider
  const defaultId = DEFAULT_MODELS[provider];
  if (defaultId) {
    try {
      return getModel(knownProvider, defaultId as never) as Model<Api>;
    } catch {
      // Fall through to try listing models
    }
  }

  // Fall back to first available model for the provider
  try {
    const models = getModels(knownProvider);
    if (models.length > 0) {
      return models[0] as Model<Api>;
    }
  } catch {
    // Provider not recognized
  }

  return null;
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/**
 * Create and configure a setup agent that can help users configure remote servers.
 *
 * The returned Agent is ready to receive prompts. Subscribe to events for UI updates.
 *
 * @example
 * ```typescript
 * const agent = await createSetupAgent({
 *   serverId: "abc-123",
 *   provider: "anthropic",
 *   apiKey: "sk-...",
 *   onMessage: (text) => console.log(text),
 *   onCommandRun: (cmd, out) => console.log(cmd, out),
 *   onConfirmNeeded: (cmd) => window.confirm(`Run: ${cmd}?`),
 * });
 *
 * agent.subscribe((event) => { ... });
 * await agent.prompt("Set up this server for Claude Code");
 * ```
 */
export async function createSetupAgent(options: SetupAgentOptions): Promise<Agent> {
  const model = resolveModel(options.provider);

  if (!model) {
    console.warn(
      `[setupAgent] Could not resolve a model for provider "${options.provider}". ` +
        `The agent will be created but LLM calls will fail until a valid model is set.`,
    );
  }

  const agent = new Agent({
    getApiKey: (provider: string) => {
      // Return the user-provided key for the matching provider
      if (provider === options.provider) {
        return options.apiKey;
      }
      return undefined;
    },
    toolExecution: "sequential",
  });

  // Configure
  agent.setSystemPrompt(SYSTEM_PROMPT);

  if (model) {
    agent.setModel(model);
  }

  agent.setTools([
    createRunRemoteCommandTool(options),
    createGetServerInfoTool(options),
  ]);

  // Subscribe to events to forward text to the caller
  agent.subscribe((event: AgentEvent) => {
    if (event.type === "message_update" && event.assistantMessageEvent.type === "text_delta") {
      options.onMessage(event.assistantMessageEvent.delta);
    }
  });

  return agent;
}
