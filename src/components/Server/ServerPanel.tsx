import { useEffect, useState } from "react";
import { useServerStore } from "../../stores/serverStore";
import { ServerList } from "../Sidebar/ServerList";
import { AddServerDialog } from "../Sidebar/AddServerDialog";

export function ServerPanel() {
  const servers = useServerStore((s) => s.servers);
  const loadServers = useServerStore((s) => s.loadServers);
  const [addOpen, setAddOpen] = useState(false);

  useEffect(() => {
    loadServers();
  }, [loadServers]);

  return (
    <div className="flex flex-1 flex-col overflow-y-auto p-4">
      {servers.length === 0 ? (
        <div className="flex flex-1 flex-col items-center justify-center text-center">
          <p className="text-sm text-zinc-400">No servers yet</p>
          <p className="mt-2 text-xs text-zinc-600">
            Connect your own server to fire tasks remotely via SSH.
          </p>
          <button
            onClick={() => setAddOpen(true)}
            className="mt-4 rounded bg-accent px-4 py-2 text-sm font-medium text-white hover:bg-accent-hover"
          >
            + Add Server
          </button>
        </div>
      ) : (
        <div className="mb-3 flex justify-end">
          <button
            onClick={() => setAddOpen(true)}
            className="rounded bg-accent px-3 py-1.5 text-xs font-medium text-white hover:bg-accent-hover"
          >
            + Add Server
          </button>
        </div>
      )}
      <ServerList />
      <AddServerDialog open={addOpen} onClose={() => setAddOpen(false)} />
    </div>
  );
}
