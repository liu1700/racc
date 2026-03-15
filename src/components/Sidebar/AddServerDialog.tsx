import { useState, useEffect } from "react";
import { useServerStore } from "../../stores/serverStore";
import type { Server, ServerConfig, SshConfigHost } from "../../types/server";

interface AddServerDialogProps {
  open: boolean;
  onClose: () => void;
  editServer?: Server;
}

type ConnectionMode = "ssh_config" | "manual";
type AuthMethod = "key" | "agent";

export function AddServerDialog({ open, onClose, editServer }: AddServerDialogProps) {
  const addServer = useServerStore((s) => s.addServer);
  const updateServer = useServerStore((s) => s.updateServer);
  const testConnection = useServerStore((s) => s.testConnection);
  const listSshConfigHosts = useServerStore((s) => s.listSshConfigHosts);

  const [name, setName] = useState("");
  const [mode, setMode] = useState<ConnectionMode>("ssh_config");

  // SSH Config mode
  const [sshHosts, setSshHosts] = useState<SshConfigHost[]>([]);
  const [selectedHost, setSelectedHost] = useState("");

  // Manual mode
  const [host, setHost] = useState("");
  const [port, setPort] = useState("22");
  const [username, setUsername] = useState("");
  const [authMethod, setAuthMethod] = useState<AuthMethod>("agent");
  const [keyPath, setKeyPath] = useState("");

  // AI Setup
  const [aiProvider, setAiProvider] = useState("");
  const [aiApiKey, setAiApiKey] = useState("");

  const [saving, setSaving] = useState(false);
  const [testing, setTesting] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [testResult, setTestResult] = useState<string | null>(null);

  // Load SSH config hosts on open
  useEffect(() => {
    if (!open) return;
    listSshConfigHosts()
      .then(setSshHosts)
      .catch(() => setSshHosts([]));
  }, [open, listSshConfigHosts]);

  // Populate fields when editing
  useEffect(() => {
    if (!open) return;
    if (editServer) {
      setName(editServer.name);
      if (editServer.auth_method === "ssh_config") {
        setMode("ssh_config");
        setSelectedHost(editServer.ssh_config_host ?? "");
      } else {
        setMode("manual");
        setHost(editServer.host);
        setPort(String(editServer.port));
        setUsername(editServer.username);
        setAuthMethod(editServer.auth_method === "key" ? "key" : "agent");
        setKeyPath(editServer.key_path ?? "");
      }
      setAiProvider(editServer.ai_provider ?? "");
      setAiApiKey(editServer.ai_api_key ?? "");
    } else {
      setName("");
      setMode("ssh_config");
      setSelectedHost("");
      setHost("");
      setPort("22");
      setUsername("");
      setAuthMethod("agent");
      setKeyPath("");
      setAiProvider("");
      setAiApiKey("");
    }
    setError(null);
    setTestResult(null);
  }, [open, editServer]);

  if (!open) return null;

  const buildConfig = (): ServerConfig => {
    if (mode === "ssh_config") {
      const matched = sshHosts.find((h) => h.host === selectedHost);
      return {
        name: name || selectedHost,
        host: matched?.hostname ?? selectedHost,
        port: matched?.port ?? 22,
        username: matched?.user ?? "root",
        auth_method: "ssh_config",
        ssh_config_host: selectedHost,
        key_path: matched?.identity_file ?? undefined,
        ai_provider: aiProvider || undefined,
        ai_api_key: aiApiKey || undefined,
      };
    }
    return {
      name: name || host,
      host,
      port: parseInt(port, 10) || 22,
      username: username || "root",
      auth_method: authMethod === "key" ? "key" : "agent",
      key_path: authMethod === "key" ? keyPath : undefined,
      ai_provider: aiProvider || undefined,
      ai_api_key: aiApiKey || undefined,
    };
  };

  const handleSave = async () => {
    setSaving(true);
    setError(null);
    try {
      const config = buildConfig();
      if (editServer) {
        await updateServer(editServer.id, config);
      } else {
        await addServer(config);
      }
      onClose();
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setSaving(false);
    }
  };

  const handleTest = async () => {
    setTesting(true);
    setError(null);
    setTestResult(null);
    try {
      // Must save first to get a server ID for testing
      const config = buildConfig();
      let serverId: string;
      if (editServer) {
        await updateServer(editServer.id, config);
        serverId = editServer.id;
      } else {
        const server = await addServer(config);
        serverId = server.id;
      }
      const result = await testConnection(serverId);
      setTestResult(result);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setTesting(false);
    }
  };

  const isValid =
    mode === "ssh_config"
      ? !!selectedHost
      : !!host && !!username;

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/60"
      onKeyDown={(e) => e.key === "Escape" && onClose()}
    >
      <div className="w-96 max-h-[90vh] overflow-y-auto rounded-lg border border-surface-3 bg-surface-1 p-5 shadow-2xl">
        <h2 className="mb-4 text-sm font-semibold text-zinc-200">
          {editServer ? "Edit Server" : "Add Server"}
        </h2>

        {/* Name */}
        <label className="mb-3 block">
          <span className="mb-1 block text-xs text-zinc-400">Name</span>
          <input
            type="text"
            value={name}
            onChange={(e) => setName(e.target.value)}
            placeholder="My Server"
            className="w-full rounded border border-surface-3 bg-surface-0 px-2 py-1.5 text-xs text-zinc-200 placeholder-zinc-600 focus:border-accent focus:outline-none"
          />
        </label>

        {/* Connection mode toggle */}
        <div className="mb-3 flex gap-1 rounded bg-surface-0 p-0.5">
          <button
            type="button"
            onClick={() => setMode("ssh_config")}
            className={`flex-1 rounded px-2 py-1 text-xs font-medium transition-colors ${
              mode === "ssh_config"
                ? "bg-surface-2 text-zinc-200"
                : "text-zinc-500 hover:text-zinc-300"
            }`}
          >
            From SSH Config
          </button>
          <button
            type="button"
            onClick={() => setMode("manual")}
            className={`flex-1 rounded px-2 py-1 text-xs font-medium transition-colors ${
              mode === "manual"
                ? "bg-surface-2 text-zinc-200"
                : "text-zinc-500 hover:text-zinc-300"
            }`}
          >
            Manual
          </button>
        </div>

        {/* SSH Config mode */}
        {mode === "ssh_config" && (
          <label className="mb-3 block">
            <span className="mb-1 block text-xs text-zinc-400">SSH Config Host</span>
            <select
              value={selectedHost}
              onChange={(e) => setSelectedHost(e.target.value)}
              className="w-full rounded border border-surface-3 bg-surface-0 px-2 py-1.5 text-xs text-zinc-200 focus:border-accent focus:outline-none"
            >
              <option value="">Select a host...</option>
              {sshHosts.map((h) => (
                <option key={h.host} value={h.host}>
                  {h.host}
                  {h.hostname ? ` (${h.hostname})` : ""}
                </option>
              ))}
            </select>
          </label>
        )}

        {/* Manual mode */}
        {mode === "manual" && (
          <>
            <label className="mb-3 block">
              <span className="mb-1 block text-xs text-zinc-400">Host</span>
              <input
                type="text"
                value={host}
                onChange={(e) => setHost(e.target.value)}
                placeholder="192.168.1.100"
                className="w-full rounded border border-surface-3 bg-surface-0 px-2 py-1.5 text-xs text-zinc-200 placeholder-zinc-600 focus:border-accent focus:outline-none"
              />
            </label>
            <div className="mb-3 flex gap-2">
              <label className="flex-1">
                <span className="mb-1 block text-xs text-zinc-400">Port</span>
                <input
                  type="number"
                  value={port}
                  onChange={(e) => setPort(e.target.value)}
                  className="w-full rounded border border-surface-3 bg-surface-0 px-2 py-1.5 text-xs text-zinc-200 focus:border-accent focus:outline-none"
                />
              </label>
              <label className="flex-1">
                <span className="mb-1 block text-xs text-zinc-400">Username</span>
                <input
                  type="text"
                  value={username}
                  onChange={(e) => setUsername(e.target.value)}
                  placeholder="root"
                  className="w-full rounded border border-surface-3 bg-surface-0 px-2 py-1.5 text-xs text-zinc-200 placeholder-zinc-600 focus:border-accent focus:outline-none"
                />
              </label>
            </div>

            {/* Auth method */}
            <div className="mb-3">
              <span className="mb-1 block text-xs text-zinc-400">Auth Method</span>
              <div className="flex gap-1 rounded bg-surface-0 p-0.5">
                <button
                  type="button"
                  onClick={() => setAuthMethod("agent")}
                  className={`flex-1 rounded px-2 py-1 text-xs font-medium transition-colors ${
                    authMethod === "agent"
                      ? "bg-surface-2 text-zinc-200"
                      : "text-zinc-500 hover:text-zinc-300"
                  }`}
                >
                  SSH Agent
                </button>
                <button
                  type="button"
                  onClick={() => setAuthMethod("key")}
                  className={`flex-1 rounded px-2 py-1 text-xs font-medium transition-colors ${
                    authMethod === "key"
                      ? "bg-surface-2 text-zinc-200"
                      : "text-zinc-500 hover:text-zinc-300"
                  }`}
                >
                  SSH Key
                </button>
              </div>
            </div>

            {authMethod === "key" && (
              <label className="mb-3 block">
                <span className="mb-1 block text-xs text-zinc-400">Key Path</span>
                <input
                  type="text"
                  value={keyPath}
                  onChange={(e) => setKeyPath(e.target.value)}
                  placeholder="~/.ssh/id_ed25519"
                  className="w-full rounded border border-surface-3 bg-surface-0 px-2 py-1.5 text-xs text-zinc-200 placeholder-zinc-600 focus:border-accent focus:outline-none"
                />
              </label>
            )}
          </>
        )}

        {/* AI Setup (optional) */}
        <details className="mb-3">
          <summary className="cursor-pointer text-xs text-zinc-400 hover:text-zinc-300">
            AI Setup Assistant (optional)
          </summary>
          <div className="mt-2 space-y-2 rounded border border-surface-3 bg-surface-0 p-3">
            <label className="block">
              <span className="mb-1 block text-xs text-zinc-400">Provider</span>
              <select
                value={aiProvider}
                onChange={(e) => setAiProvider(e.target.value)}
                className="w-full rounded border border-surface-3 bg-surface-1 px-2 py-1.5 text-xs text-zinc-200 focus:border-accent focus:outline-none"
              >
                <option value="">None</option>
                <option value="openrouter">OpenRouter</option>
                <option value="anthropic">Anthropic</option>
                <option value="openai">OpenAI</option>
              </select>
            </label>
            {aiProvider && (
              <label className="block">
                <span className="mb-1 block text-xs text-zinc-400">API Key</span>
                <input
                  type="password"
                  value={aiApiKey}
                  onChange={(e) => setAiApiKey(e.target.value)}
                  placeholder="sk-..."
                  className="w-full rounded border border-surface-3 bg-surface-1 px-2 py-1.5 text-xs text-zinc-200 placeholder-zinc-600 focus:border-accent focus:outline-none"
                />
              </label>
            )}
          </div>
        </details>

        {/* Error / Test result */}
        {error && (
          <p className="mb-3 rounded bg-red-500/10 px-3 py-2 text-xs text-red-400">
            {error}
          </p>
        )}
        {testResult && (
          <p className="mb-3 rounded bg-green-500/10 px-3 py-2 text-xs text-green-400">
            {testResult}
          </p>
        )}

        {/* Actions */}
        <div className="flex items-center justify-between">
          <button
            type="button"
            onClick={handleTest}
            disabled={!isValid || testing}
            className="rounded border border-surface-3 px-3 py-1.5 text-xs text-zinc-400 transition-colors hover:bg-surface-2 hover:text-zinc-200 disabled:opacity-50"
          >
            {testing ? "Testing..." : "Test Connection"}
          </button>
          <div className="flex gap-2">
            <button
              type="button"
              onClick={onClose}
              className="rounded px-3 py-1.5 text-xs text-zinc-400 hover:text-zinc-200"
            >
              Cancel
            </button>
            <button
              type="button"
              onClick={handleSave}
              disabled={!isValid || saving}
              className="rounded bg-accent px-3 py-1.5 text-xs font-medium text-white hover:opacity-90 disabled:opacity-50"
            >
              {saving ? "Saving..." : editServer ? "Save" : "Add"}
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}
