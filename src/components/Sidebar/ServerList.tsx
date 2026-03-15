import { useState } from "react";
import { useServerStore } from "../../stores/serverStore";
import { AddServerDialog } from "./AddServerDialog";
import { SetupWizard } from "../SetupWizard/SetupWizard";
import type { Server } from "../../types/server";

const setupStatusColor: Record<string, string> = {
  ready: "bg-status-running",
  pending: "bg-zinc-500",
  partial: "bg-yellow-500",
  error: "bg-status-error",
};

export function ServerList() {
  const servers = useServerStore((s) => s.servers);
  const connectServer = useServerStore((s) => s.connectServer);
  const disconnectServer = useServerStore((s) => s.disconnectServer);
  const removeServer = useServerStore((s) => s.removeServer);
  const loadServers = useServerStore((s) => s.loadServers);

  const [addDialogOpen, setAddDialogOpen] = useState(false);
  const [editServer, setEditServer] = useState<Server | undefined>(undefined);
  const [expandedId, setExpandedId] = useState<string | null>(null);
  const [actionLoading, setActionLoading] = useState<string | null>(null);
  const [setupServerId, setSetupServerId] = useState<string | null>(null);

  const handleAction = async (label: string, action: () => Promise<void>) => {
    setActionLoading(label);
    try {
      await action();
      await loadServers();
    } catch {
      // errors are handled in store
    } finally {
      setActionLoading(null);
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
      {servers.map((server) => (
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
              className={`h-1.5 w-1.5 flex-shrink-0 rounded-full ${
                setupStatusColor[server.setup_status] ?? "bg-zinc-500"
              }`}
              title={server.setup_status}
            />
            <span className="flex-1 truncate text-xs text-zinc-300">
              {server.name}
            </span>
            <span className="text-[10px] text-zinc-600">
              {server.host}
            </span>
          </div>

          {/* Expanded actions */}
          {expandedId === server.id && (
            <div className="ml-4 flex flex-wrap gap-1 px-2 py-1">
              <button
                onClick={() =>
                  handleAction("connect", () => connectServer(server.id))
                }
                disabled={actionLoading !== null}
                className="rounded border border-surface-3 px-2 py-0.5 text-[10px] text-zinc-400 transition-colors hover:bg-surface-2 hover:text-zinc-200 disabled:opacity-50"
              >
                {actionLoading === "connect" ? "..." : "Connect"}
              </button>
              <button
                onClick={() =>
                  handleAction("disconnect", () => disconnectServer(server.id))
                }
                disabled={actionLoading !== null}
                className="rounded border border-surface-3 px-2 py-0.5 text-[10px] text-zinc-400 transition-colors hover:bg-surface-2 hover:text-zinc-200 disabled:opacity-50"
              >
                {actionLoading === "disconnect" ? "..." : "Disconnect"}
              </button>
              <button
                onClick={() => setSetupServerId(server.id)}
                className="rounded border border-surface-3 px-2 py-0.5 text-[10px] text-zinc-400 transition-colors hover:bg-surface-2 hover:text-zinc-200"
              >
                Setup
              </button>
              <button
                onClick={() => {
                  setEditServer(server);
                  setAddDialogOpen(true);
                }}
                className="rounded border border-surface-3 px-2 py-0.5 text-[10px] text-zinc-400 transition-colors hover:bg-surface-2 hover:text-zinc-200"
              >
                Edit
              </button>
              <button
                onClick={() =>
                  handleAction("remove", () => removeServer(server.id))
                }
                disabled={actionLoading !== null}
                className="rounded border border-surface-3 px-2 py-0.5 text-[10px] text-zinc-400 transition-colors hover:bg-surface-2 hover:text-red-400 disabled:opacity-50"
              >
                {actionLoading === "remove" ? "..." : "Remove"}
              </button>
            </div>
          )}
        </div>
      ))}

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

      {setupServerId && (
        <SetupWizard
          serverId={setupServerId}
          onDone={() => setSetupServerId(null)}
        />
      )}
    </div>
  );
}
