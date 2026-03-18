import { transport } from "./transport";

export const ptyManager = {
  async write(sessionId: number, data: string): Promise<void> {
    const encoder = new TextEncoder();
    await transport.call("transport_write", {
      sessionId,
      data: Array.from(encoder.encode(data)),
    });
  },

  async resize(sessionId: number, cols: number, rows: number): Promise<void> {
    await transport.call("transport_resize", { sessionId, cols, rows });
  },

  async getBuffer(sessionId: number): Promise<Uint8Array> {
    const data = await transport.call("transport_get_buffer", { sessionId }) as number[];
    return new Uint8Array(data);
  },
};
