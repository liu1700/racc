export interface Server {
  id: string;
  name: string;
  host: string;
  port: number;
  username: string;
  auth_method: "key" | "ssh_config" | "agent";
  key_path: string | null;
  ssh_config_host: string | null;
  setup_status: "pending" | "ready" | "partial" | "error" | "failed";
  setup_details: string | null;
  ai_provider: string | null;
  ai_api_key: string | null;
  created_at: string;
  updated_at: string;
}

export interface ServerConfig {
  name: string;
  host: string;
  port?: number;
  username: string;
  auth_method: "key" | "ssh_config" | "agent";
  key_path?: string;
  ssh_config_host?: string;
  ai_provider?: string;
  ai_api_key?: string;
}

export interface SetupStep {
  key: string;
  label: string;
  status: "ok" | "installed" | "failed" | "skipped";
  detail: string | null;
}

export interface SetupReport {
  server_id: string;
  ok: boolean;
  steps: SetupStep[];
}

export interface SshConfigHost {
  host: string;
  hostname: string | null;
  port: number | null;
  user: string | null;
  identity_file: string | null;
}
