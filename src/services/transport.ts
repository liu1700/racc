export interface RaccTransport {
  call(method: string, params?: Record<string, unknown>): Promise<any>;
  on(event: string, handler: (data: any) => void): () => void;
  onTerminalData(
    sessionId: number,
    handler: (data: Uint8Array) => void,
  ): () => void;
  isLocal(): boolean;
}

// ---------------------------------------------------------------------------
// TauriTransport — wraps invoke() and listen() from @tauri-apps/api
// ---------------------------------------------------------------------------

type InvokeFn = (cmd: string, args?: Record<string, unknown>) => Promise<any>;
type ListenFn = (
  event: string,
  handler: (event: { payload: any }) => void,
) => Promise<() => void>;

let _invoke: InvokeFn | null = null;
let _listen: ListenFn | null = null;

async function getTauriInvoke(): Promise<InvokeFn> {
  if (!_invoke) {
    const mod = await import("@tauri-apps/api/core");
    _invoke = mod.invoke as InvokeFn;
  }
  return _invoke;
}

async function getTauriListen(): Promise<ListenFn> {
  if (!_listen) {
    const mod = await import("@tauri-apps/api/event");
    _listen = mod.listen as ListenFn;
  }
  return _listen;
}

class TauriTransport implements RaccTransport {
  async call(method: string, params?: Record<string, unknown>): Promise<any> {
    const invoke = await getTauriInvoke();
    return invoke(method, params);
  }

  on(event: string, handler: (data: any) => void): () => void {
    let unlistenFn: (() => void) | null = null;
    let cancelled = false;

    getTauriListen().then((listen) => {
      if (cancelled) return;
      listen(event, (e) => handler(e.payload)).then((unlisten) => {
        if (cancelled) {
          unlisten();
        } else {
          unlistenFn = unlisten;
        }
      });
    });

    return () => {
      cancelled = true;
      if (unlistenFn) unlistenFn();
    };
  }

  onTerminalData(
    sessionId: number,
    handler: (data: Uint8Array) => void,
  ): () => void {
    return this.on("transport:data", (payload: any) => {
      if (payload?.session_id === sessionId && payload?.data) {
        handler(new Uint8Array(payload.data));
      }
    });
  }

  isLocal(): boolean {
    return true;
  }
}

// ---------------------------------------------------------------------------
// WebSocketTransport — connects to ws://<host>/ws
// ---------------------------------------------------------------------------

interface PendingRequest {
  resolve: (value: any) => void;
  reject: (reason: any) => void;
}

class WebSocketTransport implements RaccTransport {
  private ws: WebSocket;
  private ready: Promise<void>;
  private nextId = 1;
  private pending = new Map<string, PendingRequest>();
  private eventHandlers = new Map<string, Set<(data: any) => void>>();
  private binaryHandlers = new Map<number, Set<(data: Uint8Array) => void>>();

  constructor(host: string) {
    const protocol = window.location.protocol === "https:" ? "wss:" : "ws:";
    this.ws = new WebSocket(`${protocol}//${host}/ws`);
    this.ws.binaryType = "arraybuffer";

    this.ready = new Promise<void>((resolve, reject) => {
      this.ws.onopen = () => resolve();
      this.ws.onerror = (e) => reject(e);
    });

    this.ws.onmessage = (msg: MessageEvent) => {
      if (msg.data instanceof ArrayBuffer) {
        this.handleBinary(msg.data);
      } else {
        this.handleText(msg.data as string);
      }
    };
  }

  private handleBinary(buffer: ArrayBuffer): void {
    if (buffer.byteLength < 8) return;

    const view = new DataView(buffer);
    const sessionId = Number(view.getBigInt64(0, true));
    const terminalData = new Uint8Array(buffer, 8);

    const handlers = this.binaryHandlers.get(sessionId);
    if (handlers) {
      for (const handler of handlers) {
        handler(terminalData);
      }
    }
  }

  private handleText(raw: string): void {
    let parsed: any;
    try {
      parsed = JSON.parse(raw);
    } catch {
      return;
    }

    // JSON-RPC response: has "id" field
    if (parsed.id != null) {
      const pending = this.pending.get(String(parsed.id));
      if (pending) {
        this.pending.delete(String(parsed.id));
        if (parsed.error) {
          pending.reject(parsed.error);
        } else {
          pending.resolve(parsed.result);
        }
      }
      return;
    }

    // Push event: has "event" field, no "id"
    if (parsed.event) {
      const handlers = this.eventHandlers.get(parsed.event);
      if (handlers) {
        for (const handler of handlers) {
          handler(parsed.data);
        }
      }
    }
  }

  async call(method: string, params?: Record<string, unknown>): Promise<any> {
    await this.ready;

    const id = String(this.nextId++);
    const request = { id, method, params: params ?? {} };

    return new Promise<any>((resolve, reject) => {
      this.pending.set(id, { resolve, reject });
      this.ws.send(JSON.stringify(request));
    });
  }

  on(event: string, handler: (data: any) => void): () => void {
    let handlers = this.eventHandlers.get(event);
    if (!handlers) {
      handlers = new Set();
      this.eventHandlers.set(event, handlers);
    }
    handlers.add(handler);

    return () => {
      handlers!.delete(handler);
      if (handlers!.size === 0) {
        this.eventHandlers.delete(event);
      }
    };
  }

  onTerminalData(
    sessionId: number,
    handler: (data: Uint8Array) => void,
  ): () => void {
    let handlers = this.binaryHandlers.get(sessionId);
    if (!handlers) {
      handlers = new Set();
      this.binaryHandlers.set(sessionId, handlers);
    }
    handlers.add(handler);

    return () => {
      handlers!.delete(handler);
      if (handlers!.size === 0) {
        this.binaryHandlers.delete(sessionId);
      }
    };
  }

  isLocal(): boolean {
    return false;
  }
}

// ---------------------------------------------------------------------------
// Auto-detection
// ---------------------------------------------------------------------------

function createTransport(): RaccTransport {
  if (
    typeof window !== "undefined" &&
    (window as any).__TAURI_INTERNALS__
  ) {
    return new TauriTransport();
  }
  return new WebSocketTransport(window.location.host);
}

export const transport = createTransport();
