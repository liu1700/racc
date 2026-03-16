import { useEffect } from "react";
import { useServerStore } from "../../stores/serverStore";
import { ServerList } from "../Sidebar/ServerList";

export function ServerPanel() {
  const servers = useServerStore((s) => s.servers);
  const loadServers = useServerStore((s) => s.loadServers);

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
        </div>
      ) : null}
      <ServerList />
    </div>
  );
}
