import { useState } from "react";
import { useServerStore } from "../../stores/serverStore";
import { AddServerDialog } from "./AddServerDialog";
import type { Server, SetupReport } from "../../types/server";

const STATUS_ICON: Record<string, { ch: string; cls: string }> = {
  ok: { ch: "✓", cls: "text-green-400" },
  installed: { ch: "✓", cls: "text-green-400" },
  failed: { ch: "✗", cls: "text-red-400" },
  skipped: { ch: "–", cls: "text-zinc-500" },
};

function statusDot(status: string): string {
  if (status === "ready") return "bg-green-500";
  if (status === "failed" || status === "error") return "bg-red-500";
  return "bg-zinc-500";
}

export function ServerList() {
  const servers = useServerStore((s) => s.servers);
  const removeServer = useServerStore((s) => s.removeServer);
  const setupServer = useServerStore((s) => s.setupServer);
  const loadServers = useServerStore((s) => s.loadServers);

  const [addDialogOpen, setAddDialogOpen] = useState(false);
  const [editServer, setEditServer] = useState<Server | undefined>(undefined);
  const [expandedId, setExpandedId] = useState<string | null>(null);
  const [removing, setRemoving] = useState(false);

  const [setupRunningId, setSetupRunningId] = useState<string | null>(null);
  const [reports, setReports] = useState<Record<string, SetupReport>>({});
  const [setupErrors, setSetupErrors] = useState<Record<string, string>>({});

  const handleRemove = async (serverId: string) => {
    setRemoving(true);
    try {
      await removeServer(serverId);
      await loadServers();
      setExpandedId(null);
    } catch {
      // errors handled in store
    } finally {
      setRemoving(false);
    }
  };

  const handleSetup = async (serverId: string) => {
    setSetupRunningId(serverId);
    setSetupErrors((prev) => ({ ...prev, [serverId]: "" }));
    try {
      const report = await setupServer(serverId);
      setReports((prev) => ({ ...prev, [serverId]: report }));
    } catch (err) {
      setSetupErrors((prev) => ({
        ...prev,
        [serverId]: err instanceof Error ? err.message : String(err),
      }));
    } finally {
      setSetupRunningId(null);
    }
  };

  return (
    <div className="px-1 py-1">
      {/* Header */}
      <div className="group flex items-center rounded px-2 py-1.5">
        <span className="flex-1 text-[10px] font-semibold uppercase tracking-wider text-zinc-500">
          Servers
        </span>
        <button
          onClick={() => {
            setEditServer(undefined);
            setAddDialogOpen(true);
          }}
          className="rounded px-1 text-xs text-zinc-500 transition-colors duration-150 hover:text-accent"
          title="Add server"
        >
          +
        </button>
      </div>

      {/* Server items */}
      {servers.map((server) => {
        const report = reports[server.id];
        const setupError = setupErrors[server.id];
        const running = setupRunningId === server.id;
        return (
          <div key={server.id} className="mb-0.5">
            <div
              onClick={() =>
                setExpandedId(expandedId === server.id ? null : server.id)
              }
              className={`group flex cursor-pointer items-center gap-2 rounded px-2 py-1 transition-colors duration-150 ${
                expandedId === server.id ? "bg-surface-3" : "hover:bg-surface-2"
              }`}
            >
              <span
                className={`h-1.5 w-1.5 flex-shrink-0 rounded-full ${statusDot(
                  server.setup_status,
                )}`}
                title={`setup: ${server.setup_status}`}
              />
              <span className="flex-1 truncate text-xs text-zinc-300">
                {server.name}
              </span>
              <span className="text-[10px] text-zinc-600">{server.host}</span>
            </div>

            {/* Expanded: Setup & Install + Remove */}
            {expandedId === server.id && (
              <div className="ml-4 px-2 py-1">
                <div className="flex items-center gap-2">
                  <span className="flex-1 text-[10px] text-zinc-600">
                    {server.username}@{server.host}:{server.port}
                  </span>
                  <button
                    onClick={() => handleSetup(server.id)}
                    disabled={running}
                    className="rounded border border-surface-3 px-2 py-0.5 text-[10px] text-zinc-300 transition-colors hover:bg-surface-2 hover:text-accent disabled:opacity-50"
                  >
                    {running ? "Setting up…" : "⚡ Setup & Install"}
                  </button>
                  <button
                    onClick={() => handleRemove(server.id)}
                    disabled={removing}
                    className="rounded border border-surface-3 px-2 py-0.5 text-[10px] text-zinc-400 transition-colors hover:bg-surface-2 hover:text-red-400 disabled:opacity-50"
                  >
                    {removing ? "..." : "Remove"}
                  </button>
                </div>

                {running && (
                  <p className="mt-2 text-[10px] text-zinc-500">
                    Connecting and installing dependencies — this can take a minute.
                  </p>
                )}

                {setupError && (
                  <p className="mt-2 rounded bg-red-500/10 px-2 py-1 text-[10px] text-red-400">
                    {setupError}
                  </p>
                )}

                {report && (
                  <div className="mt-2 rounded border border-surface-3 bg-surface-0 p-2">
                    <p
                      className={`mb-1.5 text-[11px] font-medium ${
                        report.ok ? "text-green-400" : "text-amber-400"
                      }`}
                    >
                      {report.ok
                        ? "✓ Server ready for sessions"
                        : "Setup finished with problems"}
                    </p>
                    <ul className="space-y-1">
                      {report.steps.map((s) => {
                        const icon = STATUS_ICON[s.status] ?? STATUS_ICON.skipped;
                        return (
                          <li
                            key={s.key}
                            className="flex items-start gap-2 text-[11px]"
                          >
                            <span
                              className={`mt-px w-3 shrink-0 text-center ${icon.cls}`}
                            >
                              {icon.ch}
                            </span>
                            <span className="flex-1">
                              <span className="text-zinc-300">{s.label}</span>
                              {s.status === "installed" && (
                                <span className="ml-1 text-[9px] uppercase tracking-wide text-green-500/70">
                                  installed
                                </span>
                              )}
                              {s.status === "skipped" && (
                                <span className="ml-1 text-[9px] uppercase tracking-wide text-zinc-600">
                                  skipped
                                </span>
                              )}
                              {s.detail && (
                                <span className="block break-all text-zinc-600">
                                  {s.detail}
                                </span>
                              )}
                            </span>
                          </li>
                        );
                      })}
                    </ul>
                  </div>
                )}
              </div>
            )}
          </div>
        );
      })}

      {servers.length === 0 && (
        <p className="px-2 py-1 text-[10px] text-zinc-600">
          No servers added yet.
        </p>
      )}

      <AddServerDialog
        open={addDialogOpen}
        onClose={() => {
          setAddDialogOpen(false);
          setEditServer(undefined);
        }}
        editServer={editServer}
      />
    </div>
  );
}
