# WebSocket Remote API

[< Home](Home.md) | [< Technical Architecture](Technical-Architecture.md)

Racc currently has two WebSocket surfaces. They share the same request/event envelope but differ in scope and terminal support.

| Surface | Address | Intended use | Scope |
|---------|---------|--------------|-------|
| Headless server | `ws://<host>:9399/ws` | Browser UI and trusted automation on the server host | Full shared-core command set, events, binary terminal I/O |
| Desktop compatibility server | `ws://127.0.0.1:9399` | Local external automation while Tauri is running | Smaller text-only task/planner/session API |

Neither surface is a versioned public API yet. Clients should expect method additions or response-shape cleanup before a stable API release.

## Security

`racc-server` binds to `0.0.0.0` and currently has no application-level authentication, authorization, origin enforcement, or TLS. Use it only on localhost or a trusted private network such as Tailscale. Do not publish port 9399 to the public internet.

The Tauri compatibility server binds to localhost only and also has no authentication. Local processes with access to it can create/stop sessions and mutate tasks.

## Text Protocol

Requests and responses are JSON text frames.

### Request

```json
{
  "id": "req-1",
  "method": "list_repos",
  "params": {}
}
```

### Success

```json
{
  "id": "req-1",
  "result": []
}
```

### Error

```json
{
  "id": "req-1",
  "error": "Unknown method: example"
}
```

### Push Event

Events have no request ID:

```json
{
  "event": "session_status_changed",
  "data": {
    "session_id": 3,
    "status": "Completed",
    "pr_url": "https://github.com/example/repo/pull/42"
  }
}
```

Method parameters are snake_case on the wire. The built-in browser `WebSocketTransport` automatically converts top-level camelCase frontend arguments.

## Headless `/ws` Methods

The headless server maps these calls to `racc-core`. Optional parameters are marked with `?`.

### Sessions and Repositories

| Method | Parameters |
|--------|------------|
| `list_repos` | none |
| `import_repo` | `path` |
| `remove_repo` | `repo_id` |
| `create_session` | `repo_id`, `use_worktree`, `branch?`, `agent?`, `task_description?`, `server_id?`, `skip_permissions?` |
| `stop_session` | `session_id` |
| `reattach_session` | `session_id`, `skip_permissions?` |
| `reconnect_session` | `session_id` |
| `remove_session` | `session_id`, `delete_worktree` |
| `update_session_pr_url` | `session_id`, `pr_url` |
| `reconcile_sessions` | none |
| `get_session_diff` | `session_id` |
| `sync` | none; returns current repository/session state |

Accepted agent values used by the UI are `claude-code` and `codex`.

### Tasks and Attachments

| Method | Parameters |
|--------|------------|
| `create_task` | `repo_id`, `description`, `images?` (JSON string) |
| `list_tasks` | `repo_id` |
| `update_task_status` | `task_id`, `status`, `session_id?` |
| `update_task_description` | `task_id`, `description` |
| `update_task_images` | `task_id`, `images` |
| `delete_task` | `task_id` |
| `save_task_image` | `repo_path`, `filename`, `data` (base64) |
| `copy_file_to_task_images` | `repo_path`, `source_path`, `filename` |
| `rename_task_image` | `repo_path`, `old_name`, `new_name` |
| `delete_task_image` | `repo_path`, `filename` |

### Task Planner

| Method | Parameters |
|--------|------------|
| `get_latest_task_plan` | `repo_id` |
| `start_task_plan` | `repo_id`, `source_input`, `agent` |
| `confirm_task_plan` | `run_id`, `selected_keys` (string array) |

### Merge Manager

| Method | Parameters |
|--------|------------|
| `get_merge_manager` | `repo_id` |
| `reset_merge_manager` | `repo_id` |
| `set_task_ready_to_merge` | `task_id`, `ready` |
| `update_merge_settings` | `repo_id`, `target_branch`, `agent`, `instructions` |
| `start_merge_run` | `repo_id` |
| `resolve_merge_run` | `run_id`, `status` (`succeeded` or `failed`) |
| `retry_merge_run` | `run_id` |

### Test Manager

| Method | Parameters |
|--------|------------|
| `get_test_manager` | `repo_id` |
| `reset_test_manager` | `repo_id` |
| `update_test_settings` | `repo_id`, `target_branch`, `agent`, `instructions` |
| `start_test_run` | `repo_id` |
| `resolve_test_run` | `run_id`, `status` (`succeeded` or `failed`) |
| `retry_test_run` | `run_id` |

### Terminal Transport

| Method | Parameters / result |
|--------|---------------------|
| `transport_write` | `session_id`, `data` as byte array or base64-compatible input |
| `transport_resize` | `session_id`, `cols`, `rows` |
| `transport_get_buffer` | `session_id`; returns base64 terminal buffer |
| `transport_is_alive` | `session_id`; returns boolean |

The built-in browser client normally sends terminal input as binary frames instead of calling `transport_write`.

### SSH Servers

| Method | Parameters |
|--------|------------|
| `list_servers` | none |
| `add_server` | `config` object |
| `update_server` | `server_id`, `config` object |
| `remove_server` | `server_id` |
| `connect_server` / `disconnect_server` | `server_id` |
| `test_connection` | `server_id` |
| `test_connection_config` | `config` object |
| `setup_server` | `server_id` |
| `list_ssh_config_hosts` | none |
| `execute_remote_command` | `server_id`, `command` |

### Files, Git, Usage, and Other

| Method | Parameters |
|--------|------------|
| `read_file` | `file_path`, `session_id?`, `repo_id?`, `max_lines?` |
| `search_files` | `query`, `session_id?`, `repo_id?` |
| `create_worktree` | `path`, `branch` |
| `delete_worktree` | `path` |
| `get_diff` | `worktree_path` |
| `get_project_costs` | `worktree_path` |
| `get_global_costs` | none |
| `record_session_events` | `events` array |
| `get_insights` | `status?` |
| `update_insight_status` | `id`, `status` |
| `run_batch_analysis` | none |
| `save_insight` | `insight_type`, `severity`, `title`, `summary`, `detail_json`, `fingerprint` |
| `reset_db` | none; destructive |
| `open_url` | accepted as a no-op in headless mode; browser UI opens safe URLs itself |

## Events

| Event | Data |
|-------|------|
| `session_status_changed` | `session_id`, `status`, `pr_url?` |
| `task_status_changed` | `task_id`, `status`, `session_id?` |
| `task_deleted` | `task_id` |
| `task_plan_changed` | `repo_id`, `run_id` |
| `merge_manager_changed` | `repo_id`, `run_id?` |
| `test_manager_changed` | `repo_id`, `run_id?` |

Clients should refetch the affected manager/plan state after a change event rather than reconstructing the result from the event payload.

## Binary Terminal Frames (`racc-server` Only)

Both directions use:

```text
bytes 0..7    signed i64 session_id, little endian
bytes 8..end  raw terminal bytes
```

- Server to client: output for that session.
- Client to server: terminal input for that session.
- A client frame must contain more than eight bytes.

Resize and initial buffer replay remain JSON calls.

## Desktop Compatibility Methods

The localhost Tauri server currently supports only:

- task CRUD: `create_task`, `list_tasks`, `update_task_status`, `update_task_description`, `delete_task`;
- Task Planner: `get_latest_task_plan`, `start_task_plan`, `confirm_task_plan`;
- sessions: `create_session`, `stop_session`, `reattach_session`, `reconnect_session`;
- queries: `list_repos`, `get_session_diff`.

It broadcasts the same domain events but does not stream terminal binary frames. Some legacy result envelopes wrap collections (for example `{ "repos": [...] }` or `{ "tasks": [...] }`) while the headless dispatcher serializes core return values directly. Treat the two endpoints as separate compatibility surfaces until the public protocol is versioned.

## Minimal Headless Client

```javascript
const ws = new WebSocket("ws://127.0.0.1:9399/ws");

ws.addEventListener("open", () => {
  ws.send(JSON.stringify({ id: "1", method: "list_repos", params: {} }));
});

ws.addEventListener("message", async (event) => {
  if (typeof event.data === "string") {
    console.log(JSON.parse(event.data));
  } else {
    const data = new Uint8Array(await event.data.arrayBuffer());
    const sessionId = Number(new DataView(data.buffer).getBigInt64(0, true));
    console.log("terminal", sessionId, data.subarray(8));
  }
});
```

[Next: Session Lifecycle >](Session-Lifecycle.md)
