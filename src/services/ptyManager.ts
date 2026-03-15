import { invoke } from "@tauri-apps/api/core";

export const ptyManager = {
  async write(sessionId: number, data: string): Promise<void> {
    const encoder = new TextEncoder();
    await invoke("transport_write", {
      sessionId,
      data: Array.from(encoder.encode(data)),
    });
  },

  async resize(sessionId: number, cols: number, rows: number): Promise<void> {
    await invoke("transport_resize", { sessionId, cols, rows });
  },

  async getBuffer(sessionId: number): Promise<Uint8Array> {
    const data = await invoke<number[]>("transport_get_buffer", { sessionId });
    return new Uint8Array(data);
  },
};
