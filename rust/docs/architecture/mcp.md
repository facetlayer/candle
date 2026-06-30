# MCP server

Candle exposes an MCP (Model Context Protocol) server that lets an LLM client manage local dev processes. The Rust implementation lives in [`rust/candle-core/src/mcp/mod.rs`](../../candle-core/src/mcp/mod.rs), with the output-capture layer in [`rust/candle-core/src/output.rs`](../../candle-core/src/output.rs). It mirrors the original Node implementation in `src/mcp/mcp-main.ts` and `src/mcp/ConsoleLogInterceptor.ts`, which remains the source-of-truth for exact behavior.

## 1. Overview & entry point

The server is launched by the CLI when the user runs `candle mcp` or passes the `--mcp` flag. CLI dispatch routes both forms to `serve_mcp()` (`mcp/mod.rs`), which is the whole subsystem — it opens the database, captures the current working directory, and runs the blocking server loop. It diverges (`-> !`): it never returns, exiting the process when stdin closes.

## 2. Transport

- **stdio only.** The MCP JSON-RPC protocol runs over stdin/stdout.
- **Newline-delimited JSON framing**, *not* `Content-Length`-prefixed framing: each request and response is a single line of JSON terminated by `\n`. The server loop reads `stdin.lock().lines()`, trims each line, skips empty lines, and ignores lines that do not parse as JSON. Each response is written with `writeln!` and flushed.
- **stdout purity:** only MCP protocol frames may reach real stdout. All command-handler human-readable output is routed through [`crate::output`] and captured (see §6) so nothing leaks to stdout and corrupts the JSON-RPC stream. The transport is **hand-rolled** rather than built on `rmcp`: the candle-core handlers are synchronous (rusqlite), so a blocking line reader matches the protocol exactly and keeps it under direct control.
- **stdin-close shutdown:** when stdin reaches EOF (the `lines()` iterator ends) or a read errors, the loop breaks and the process exits with `std::process::exit(0)`. There is no auto-shutdown transport to rely on; the EOF → `exit(0)` behavior is wired explicitly.

## 3. Server identity & capabilities

The `initialize` response is built directly in `handle_message`:

```json
{
  "protocolVersion": "2025-06-18",
  "capabilities": { "tools": {} },
  "serverInfo": { "name": "<CARGO_PKG_NAME>", "version": "<CARGO_PKG_VERSION>" },
  "instructions": "<SERVER_INSTRUCTIONS>"
}
```

- **Protocol version**: `2025-06-18`.
- **Server name / version**: `env!("CARGO_PKG_NAME")` / `env!("CARGO_PKG_VERSION")`, reproducing the Node server's use of the `package.json` `name`/`version` fields (currently `candle`).
- **Capabilities**: declares the `tools` capability only, as an empty object `{}`. No resources, prompts, logging, etc.
- **Instructions string** (server-level), byte-for-byte:
  > Tool for running and managing local dev servers. Use this when launching any local servers, including web servers, APIs, and other services.
- A `ping` request returns an empty result `{}`.

## 4. Request handling

`handle_message` dispatches one request and returns the JSON-RPC response envelope, or `None` for messages without an `id`.

- **Notifications / id-less messages** (e.g. `notifications/initialized`) get no response: if there is no `id`, the dispatcher returns `None` and the loop continues silently.
- **`tools/list`** returns `{ tools: [...] }`, each entry `{ name, description, inputSchema }` projected from the tool registry (the handler fn is not serialized). Order is the registry order (see §5).
- **`tools/call`**:
  1. Reads `params.name` and `params.arguments` (arguments default to `{}`).
  2. Looks up the tool by exact name match. If not found, returns a JSON-RPC error with code `-32601` (`METHOD_NOT_FOUND`) and message `Unknown tool: <name>`. This is a protocol-level error response, distinct from handler errors which return `isError: true` content.
  3. Runs the handler via `call_wrapped` (§6), which returns `{ result, error, logs }`.
  4. Builds a `content` array of `{ type: "text", text }` items via `build_call_result`:
     - **If `logs` is non-empty**: first push one text item = `logs.join("\n")`.
     - **If `error`**: push text item `Error: <message>` and return `{ content, isError: true }`.
     - **Else if a `result` value exists**: push text item = the result serialized with `serde_json::to_string_pretty` (2-space indent) and return `{ content, isError: false }`.
     - If there is no result value and no error, return `{ content, isError: false }` (content may contain only logs, or be empty).
- **Any other method** returns a `-32601` error with message `Method not found`.

Subtle ordering: logs always come **before** the result/error text item in the content array. Result is serialized as pretty (2-space) JSON. Error text is exactly `Error: <message>`.

## 5. Tool definitions (exact)

The registry `tool_definitions()` defines nine tools, in this order. Every `inputSchema.type` is `"object"`; JSON Schema property types are as written (`boolean`, `string`, `number`). Each handler runs against `(&Connection, &Path /* cwd */, &Value /* arguments */)` and returns `Result<Option<Value>, CandleError>`.

### 5.1 `ListServices`
- description: `List services with structured output`
- properties: `showAll` (boolean, "Show all services or just current directory (optional)")
- required: none
- handler: `handle_list(conn, cwd, showAll)` (mirrors the original `src/list-command.ts`) → `ListOutput`:
  ```
  { processes: { command, workingDir, uptime, pid, status, serviceName, configChanged? }[], showAll?, message? }
  ```
  `showAll=true` lists all DB processes (no config file needed, status always `RUNNING`); otherwise lists processes for the current project dir.

### 5.2 `ListPorts`
- description: `List open ports for running services`
- properties: `showAll` (boolean), `serviceName` (string, "Filter to a specific service name (optional)")
- required: none
- handler: `handle_list_ports(conn, cwd, showAll, command_names)` (mirrors `src/list-ports-command.ts`) → `ListPortsOutput`:
  ```
  { ports: { serviceName, pid, port, address, protocol, isChildProcess }[] }
  ```
  Note: the single `serviceName`, if present, is passed through as a one-element `command_names` array (empty otherwise).

### 5.3 `GetLogs`
- description: `Get recent logs for a specific service`
- properties: `name` (string), `limit` (number, "Maximum number of log lines to return (optional)"), `projectDir` (string, "Project directory where the service is defined (optional - for cross-directory access)")
- required: `["name"]`
- handler: validates `name` is present (else error `Service name is required`); resolves the project dir (from `projectDir` if given, else `find_project_dir(cwd)`); calls `handle_logs_command(conn, projectDir, [name], limit, None)`.
  - `DEFAULT_LOGS_LIMIT = 200`. The limit is nullish-defaulted: an explicit `0` passes through (the code only falls back to 200 when `limit` is absent or `null`, not when it is a falsy number).
  - `handle_logs_command` returns nothing — it **emits logs through [`crate::output`]**, so the actual log output is captured and surfaced through the response's `logs`, not through `result` (the handler returns `Ok(None)`). This is the one tool whose output flows entirely through the output-capture path.

### 5.4 `StartService`
- description: `Start a config-defined service (use StartTransientService for transient processes)`
- properties: `name` (string)
- required: `["name"]`
- handler: validate `name` (else `Service name is required`); resolve project dir; `start_one_service` with `shell: None`, `root: None` (mirrors `src/start/startOneService.ts`) → returns `{ projectDir, serviceName }`.

### 5.5 `StartTransientService`
- description: `Start a transient process with a custom shell command (not defined in config file)`
- properties: `name` (string, "Name for the transient process"), `shell` (string, "Shell command to run the service"), `root` (string, "Root directory for the service (optional, relative to project)")
- required: `["name", "shell"]`
- handler: validate `name && shell` (else `Service name and shell command are required`); resolve project dir; `start_one_service` with the given `shell` and optional `root` → returns `{ projectDir, serviceName }`.

### 5.6 `KillService`
- description: `Kill a running service`
- properties: `name` (string)
- required: `["name"]`
- handler: validate `name`; resolve project dir; `handle_kill_command(conn, projectDir, [name], …)`. **Returns nothing** (`Ok(None)`) — the response contains only captured logs (if any) with `isError: false`.

### 5.7 `RestartService`
- description: `Restart a running service. If no name provided, restarts all running services in the project.`
- properties: `name` (string, "Name of the service to restart. If not provided, restarts all running services.")
- required: none
- handler: resolve project dir; `handle_restart(conn, projectDir, names)`, where `names` is the one-element list `[name]` if provided, else empty. Empty `names` ⇒ restart all running services. Returns nothing (`Ok(None)`).

### 5.8 `AddServerConfig`
- description: `Add a new server configuration to .candle.json`
- properties: `name` (string), `shell` (string), `root` (string, "Root directory for the service (optional)")
- required: `["name", "shell"]`
- handler: validate `name && shell` (else `Service name and shell command are required`); calls `add_server_config({ name, shell, root, … }, cwd)`, then emits the returned success message once via `crate::output::out`. (The Node original double-logged this success message — once inside `addServerConfig` and again in the handler; the Rust implementation emits it a single time.) Returns nothing (`Ok(None)`).
  - `add_server_config` finds-or-creates `.candle.json` (default filename `.candle.json`), rejects duplicate service names (`Service '<name>' already exists in configuration`), validates, and writes the config with 2-space-indent JSON.

### 5.9 `OpenBrowser`
- description: `Open a browser window to a running service's port` (note the literal apostrophe)
- properties: `serviceName` (string, "Name of the service to open in browser")
- required: `["serviceName"]`
- handler: validate `serviceName` present (else `Service name is required`); resolve project dir; `handle_open_browser(conn, cwd, projectDir, serviceName)` (mirrors `src/open-browser-command.ts`) → `OpenBrowserOutput` `{ serviceName, port, url }`. Spawns the platform browser opener.

## 6. Output capture (`crate::output`)

Command handlers were written for CLI use and emit human-readable output. In the MCP server those lines must not hit stdout (they would corrupt the JSON-RPC stream) and must instead be **captured and returned inside the tool response**. Rust has no global mutable `console` to monkeypatch; instead the handlers emit through `crate::output::out` / `crate::output::err`, which by default pass through to the real stdout/stderr but buffer into a thread-local when a `capture` scope is active.

Mechanism (`call_wrapped`):

```rust
let (res, captured) = crate::output::capture(|| handler(conn, cwd, args));
let logs = captured.mcp_log_lines();
match res {
    Ok(result) => CallOutcome { result, error: None, logs },
    Err(e)     => CallOutcome { result: None, error: Some(e.to_string()), logs },
}
```

`capture` installs a thread-local buffer for the duration of one handler call (handlers run synchronously on the same thread, so no cross-thread sharing is needed), restores it even on panic, and returns the handler's value alongside everything emitted.

`CapturedOutput` keeps the `stdout` and `stderr` lines separately plus a combined transcript in emission order. `mcp_log_lines()` produces the lines for the tool response, mirroring the Node `ConsoleLogInterceptor`:
- stdout lines pass through verbatim;
- stderr lines are prefixed with `"[stderr] "`;
- emission order is preserved across both streams.

A handler error is normalized to its `to_string()` message; only that message is surfaced in the response text (`Error: <message>`).

## 7. External dependencies

- **JSON-RPC / MCP transport**: hand-rolled over `serde_json` (no `rmcp`). Newline-delimited JSON on stdin/stdout, blocking line reader.
- **Database / handlers**: `rusqlite` (synchronous), reached through the candle-core command modules.
- **Browser open**: the platform browser opener spawned by the open-browser command.

JSON-RPC error codes used: `-32601` (method not found) for both unknown methods and unknown tool names.

## 8. Behaviors easy to get wrong

1. **stdout purity** — only MCP frames on stdout. The whole output-capture layer exists for this.
2. **stdin-close → exit(0)** — wired explicitly in the server loop; there is no auto-shutdown transport.
3. **Content ordering** — logs item first, then the result/error item. Result is serialized as pretty (2-space) JSON. Error text is exactly `Error: <message>` (no stack in the visible text).
4. **No-result vs JSON** — `KillService`, `RestartService`, `GetLogs`, and `AddServerConfig` return no structured result; their content is logs-only. No `"null"`/`"undefined"` text is emitted — a result item is pushed only when a value exists.
5. **Single-element name arrays** — several tools wrap one name into a one-element list; the underlying handlers expect arrays.
6. **`find_project_dir` errors** if no `.candle.json`/`.candle-setup.json` is found walking up from cwd; that error becomes an `isError: true` response (not a transport error). Config filenames, in priority order: `.candle.json`, `.candle-setup.json`.
7. **`limit` default 200** is applied with nullish semantics: an explicit `0` is passed through, only an absent/`null` `limit` falls back to 200.
8. **Unknown tool name** → `-32601` protocol-level error response, distinct from handler errors which return `isError: true` content.
