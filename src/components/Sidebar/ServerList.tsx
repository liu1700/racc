import { useState } from "react";
import { useServerStore } from "../../stores/serverStore";
import { AddServerDialog } from "./AddServerDialog";
import type { Server } from "../../types/server";

export function ServerList() {
  const servers = useServerStore((s) => s.servers);
  const removeServer = useServerStore((s) => s.removeServer);
  const loadServers = useServerStore((s) => s.loadServers);

  const [addDialogOpen, setAddDialogOpen] = useState(false);
  const [editServer, setEditServer] = useState<Server | undefined>(undefined);
  const [expandedId, setExpandedId] = useState<string | null>(null);
  const [removing, setRemoving] = useState(false);

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
              className="h-1.5 w-1.5 flex-shrink-0 rounded-full bg-zinc-500"
            />
            <span className="flex-1 truncate text-xs text-zinc-300">
              {server.name}
            </span>
            <span className="text-[10px] text-zinc-600">
              {server.host}
            </span>
          </div>

          {/* Expanded: only Remove */}
          {expandedId === server.id && (
            <div className="ml-4 flex items-center gap-2 px-2 py-1">
              <span className="flex-1 text-[10px] text-zinc-600">
                {server.username}@{server.host}:{server.port}
              </span>
              <button
                onClick={() => handleRemove(server.id)}
                disabled={removing}
                className="rounded border border-surface-3 px-2 py-0.5 text-[10px] text-zinc-400 transition-colors hover:bg-surface-2 hover:text-red-400 disabled:opacity-50"
              >
                {removing ? "..." : "Remove"}
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
    </div>
  );
}
