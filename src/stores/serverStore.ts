import { create } from "zustand";
import { transport } from "../services/transport";
import type { Server, ServerConfig, SshConfigHost } from "../types/server";

interface ServerState {
  servers: Server[];
  loading: boolean;
  error: string | null;

  loadServers: () => Promise<void>;
  addServer: (config: ServerConfig) => Promise<Server>;
  updateServer: (serverId: string, config: ServerConfig) => Promise<Server>;
  removeServer: (serverId: string) => Promise<void>;
  connectServer: (serverId: string) => Promise<void>;
  disconnectServer: (serverId: string) => Promise<void>;
  testConnection: (serverId: string) => Promise<string>;
  listSshConfigHosts: () => Promise<SshConfigHost[]>;
}

export const useServerStore = create<ServerState>((set, _get) => ({
  servers: [],
  loading: false,
  error: null,

  loadServers: async () => {
    set({ loading: true });
    try {
      const servers = await transport.call("list_servers") as Server[];
      set({ servers, loading: false });
    } catch (e) {
      set({ error: String(e), loading: false });
    }
  },

  addServer: async (config) => {
    const server = await transport.call("add_server", { config }) as Server;
    set((s) => ({ servers: [server, ...s.servers] }));
    return server;
  },

  updateServer: async (serverId, config) => {
    const server = await transport.call("update_server", { serverId, config }) as Server;
    set((s) => ({
      servers: s.servers.map((sv) => (sv.id === serverId ? server : sv)),
    }));
    return server;
  },

  removeServer: async (serverId) => {
    await transport.call("remove_server", { serverId });
    set((s) => ({ servers: s.servers.filter((sv) => sv.id !== serverId) }));
  },

  connectServer: async (serverId) => {
    await transport.call("connect_server", { serverId });
  },

  disconnectServer: async (serverId) => {
    await transport.call("disconnect_server", { serverId });
  },

  testConnection: async (serverId) => {
    return await transport.call("test_connection", { serverId }) as string;
  },

  listSshConfigHosts: async () => {
    return await transport.call("list_ssh_config_hosts") as SshConfigHost[];
  },
}));
