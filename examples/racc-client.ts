/**
 * Racc WebSocket Client SDK
 *
 * Usage:
 *   const racc = new RaccClient("ws://127.0.0.1:9399");
 *   await racc.connect();
 *
 *   // List repos
 *   const { repos } = await racc.call("list_repos");
 *
 *   // Create a task
 *   const { task_id } = await racc.call("create_task", {
 *     repo_id: repos[0].id,
 *     description: "Fix the login bug",
 *   });
 *
 *   // Create a session (starts agent automatically in Racc UI)
 *   const { session_id } = await racc.call("create_session", {
 *     repo_id: repos[0].id,
 *     use_worktree: true,
 *     branch: "fix/login-bug",
 *   });
 *
 *   // Listen for events
 *   racc.on("session_status_changed", (data) => {
 *     console.log(`Session ${data.session_id} → ${data.status}`);
 *     if (data.pr_url) console.log(`PR: ${data.pr_url}`);
 *   });
 *
 *   // Stop session when done
 *   await racc.call("stop_session", { session_id });
 *
 *   racc.close();
 */

type EventHandler = (data: Record<string, unknown>) => void;

interface WsRequest {
  id: string;
  method: string;
  params: Record<string, unknown>;
}

interface WsResponse {
  id?: string;
  result?: unknown;
  error?: string;
  event?: string;
  data?: Record<string, unknown>;
}

export class RaccClient {
  private ws: WebSocket | null = null;
  private requestId = 0;
  private pending = new Map<string, {
    resolve: (value: unknown) => void;
    reject: (reason: string) => void;
  }>();
  private listeners = new Map<string, Set<EventHandler>>();

  constructor(private url: string = "ws://127.0.0.1:9399") {}

  /** Connect to Racc WebSocket server */
  connect(): Promise<void> {
    return new Promise((resolve, reject) => {
      this.ws = new WebSocket(this.url);

      this.ws.onopen = () => resolve();
      this.ws.onerror = (e) => reject(e);

      this.ws.onmessage = (msg) => {
        const data: WsResponse = JSON.parse(msg.data);

        // Push event (no id)
        if (data.event) {
          const handlers = this.listeners.get(data.event);
          if (handlers) {
            for (const handler of handlers) {
              handler(data.data ?? {});
            }
          }
          return;
        }

        // Response to a request
        if (data.id) {
          const pending = this.pending.get(data.id);
          if (pending) {
            this.pending.delete(data.id);
            if (data.error) {
              pending.reject(data.error);
            } else {
              pending.resolve(data.result);
            }
          }
        }
      };

      this.ws.onclose = () => {
        // Reject all pending requests
        for (const [, p] of this.pending) {
          p.reject("Connection closed");
        }
        this.pending.clear();
      };
    });
  }

  /** Call a Racc method and wait for the response */
  call(method: string, params: Record<string, unknown> = {}): Promise<any> {
    return new Promise((resolve, reject) => {
      if (!this.ws || this.ws.readyState !== WebSocket.OPEN) {
        return reject("Not connected");
      }

      const id = `req_${++this.requestId}`;
      this.pending.set(id, { resolve, reject });

      const request: WsRequest = { id, method, params };
      this.ws.send(JSON.stringify(request));

      // Timeout after 30s
      setTimeout(() => {
        if (this.pending.has(id)) {
          this.pending.delete(id);
          reject(`Timeout: ${method}`);
        }
      }, 30000);
    });
  }

  /** Subscribe to push events */
  on(event: string, handler: EventHandler): () => void {
    if (!this.listeners.has(event)) {
      this.listeners.set(event, new Set());
    }
    this.listeners.get(event)!.add(handler);

    // Return unsubscribe function
    return () => this.listeners.get(event)?.delete(handler);
  }

  /** Close the connection */
  close() {
    this.ws?.close();
    this.ws = null;
  }
}

// ---- Available Methods ----
//
// Task operations:
//   create_task({ repo_id, description })          → { task_id }
//   list_tasks({ repo_id })                        → { tasks: [...] }
//   update_task_status({ task_id, status, session_id? }) → {}
//   update_task_description({ task_id, description })    → {}
//   delete_task({ task_id })                        → {}
//
// Session operations:
//   create_session({ repo_id, use_worktree, branch?, agent? }) → { session_id }
//   stop_session({ session_id })                    → {}
//   reattach_session({ session_id })                → { session }
//
// Query operations:
//   list_repos()                                    → { repos: [...] }
//   get_session_diff({ session_id })                → { diff: "..." }
//
// ---- Push Events ----
//
//   session_status_changed  → { session_id, status, pr_url? }
//   task_status_changed     → { task_id, status, session_id? }
//   task_deleted            → { task_id }


// ---- Example: CLI usage with Bun/Node ----

async function main() {
  const racc = new RaccClient();
  await racc.connect();
  console.log("Connected to Racc");

  // Listen for all session events
  racc.on("session_status_changed", (data) => {
    console.log(`[event] Session ${data.session_id} → ${data.status}`);
    if (data.pr_url) console.log(`  PR: ${data.pr_url}`);
  });

  racc.on("task_status_changed", (data) => {
    console.log(`[event] Task ${data.task_id} → ${data.status}`);
  });

  // List repos
  const { repos } = await racc.call("list_repos");
  console.log(`Found ${repos.length} repos`);

  if (repos.length === 0) {
    console.log("No repos imported. Import a repo in Racc first.");
    racc.close();
    return;
  }

  const repo = repos[0];
  console.log(`Using repo: ${repo.name} (${repo.path})`);

  // Create a task
  const { task_id } = await racc.call("create_task", {
    repo_id: repo.id,
    description: "Hello from external client!",
  });
  console.log(`Created task #${task_id}`);

  // Create a session
  const { session_id } = await racc.call("create_session", {
    repo_id: repo.id,
    use_worktree: true,
    branch: "racc/remote-test",
  });
  console.log(`Created session #${session_id} — agent should start in Racc UI`);

  // Wait a bit, then stop
  console.log("Waiting 10s then stopping session...");
  await new Promise((r) => setTimeout(r, 10000));

  await racc.call("stop_session", { session_id });
  console.log("Session stopped");

  racc.close();
}

// Run if executed directly
main().catch(console.error);
