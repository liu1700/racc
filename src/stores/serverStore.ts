import { create } from "zustand";
import { invoke } from "@tauri-apps/api/core";
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
      const servers = await invoke<Server[]>("list_servers");
      set({ servers, loading: false });
    } catch (e) {
      set({ error: String(e), loading: false });
    }
  },

  addServer: async (config) => {
    const server = await invoke<Server>("add_server", { config });
    set((s) => ({ servers: [server, ...s.servers] }));
    return server;
  },

  updateServer: async (serverId, config) => {
    const server = await invoke<Server>("update_server", { serverId, config });
    set((s) => ({
      servers: s.servers.map((sv) => (sv.id === serverId ? server : sv)),
    }));
    return server;
  },

  removeServer: async (serverId) => {
    await invoke("remove_server", { serverId });
    set((s) => ({ servers: s.servers.filter((sv) => sv.id !== serverId) }));
  },

  connectServer: async (serverId) => {
    await invoke("connect_server", { serverId });
  },

  disconnectServer: async (serverId) => {
    await invoke("disconnect_server", { serverId });
  },

  testConnection: async (serverId) => {
    return await invoke<string>("test_connection", { serverId });
  },

  listSshConfigHosts: async () => {
    return await invoke<SshConfigHost[]>("list_ssh_config_hosts");
  },
}));
