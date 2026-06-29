I have enough to write the spec.

# MCP Subsystem Specification (`src/mcp/`)

Target: exact Rust reimplementation. Source files: `/Users/andy/candle/src/mcp/mcp-main.ts`, `/Users/andy/candle/src/mcp/ConsoleLogInterceptor.ts`.

## 1. Overview & Entry Point

Candle exposes an MCP (Model Context Protocol) server that lets an LLM client manage local dev processes. It is launched by the CLI when the user runs `candle mcp` or passes the `--mcp` flag:

```
src/main-cli.ts:322   if (command === 'mcp' || mcp) {
src/main-cli.ts:323     await serveMCP();
```

`serveMCP()` (exported from `mcp-main.ts:315`) is the whole subsystem. `main()` (`mcp-main.ts:415`) just calls `serveMCP()`.

## 2. Transport

- **stdio only.** `const transport = new StdioServerTransport();` (`mcp-main.ts:401`), then `await server.connect(transport)` (`mcp-main.ts:411`).
- The MCP JSON-RPC protocol runs over stdin/stdout. **stdout must carry only MCP protocol frames** — all internal logging goes through `infoLog` (a file/db logger, not console) and console output from handlers is intercepted (see §6). This is critical: in Rust do not let anything write stray bytes to stdout.
- **Manual stdin-close shutdown** (`mcp-main.ts:405-409`):
  ```js
  process.stdin.on('close', async () => {
    infoLog('MCP: stdin closed');
    await transport.close();
    process.exit(0);
  });
  ```
  Comment notes the SDK transport does **not** auto-exit on stdin close, so this is added explicitly. The Rust impl must exit the process (code 0) when stdin reaches EOF/close.

## 3. Server Identity & Capabilities

Constructed at `mcp-main.ts:321-334`:

```js
const packageInfo = findPackageJson();
const server = new Server(
  { name: packageInfo.name, version: packageInfo.version },
  {
    capabilities: { tools: {} },
    instructions:
      'Tool for running and managing local dev servers. Use this when launching any local servers, including ' +
      'web servers, APIs, and other services.',
  }
);
```

- **Server name**: `package.json` `name` field. **Server version**: `package.json` `version` field. Currently name is `candle` (the published package). In Rust, hardcode or read from the binary's own metadata — must match the package name/version semantics.
- `findPackageJson()` (`src/findPackageJson.ts`) resolves `../package.json` then `../../package.json` relative to the module file (handles bundled `dist/` vs source `src/mcp/` layouts). In Rust use the crate name/version (e.g. `env!("CARGO_PKG_NAME")` / `CARGO_PKG_VERSION`) to reproduce.
- **Capabilities**: declares `tools` capability only (empty object `{}`). No resources, prompts, logging, etc.
- **Instructions string** (server-level): exact text above. Must be byte-for-byte preserved in the `initialize` response.

## 4. Request Handlers

Two JSON-RPC handlers are registered:

### `ListTools` (`mcp-main.ts:337-349`)
Returns `{ tools: [...] }` where each entry is `{ name, description, inputSchema }` projected from the `toolDefinitions` array (the `handler` field is stripped). Order is the array order (see §5).

### `CallTool` (`mcp-main.ts:352-398`)
1. Reads `request.params.name` and `request.params.arguments`.
2. Looks up tool by exact name match. If not found:
   ```js
   throw new McpError(ErrorCode.MethodNotFound, `Unknown tool: ${name}`);
   ```
   `ErrorCode.MethodNotFound` = JSON-RPC `-32601`.
3. Runs the handler via `callWrapped` (§6), which returns `{ result, error, logs }`.
4. Builds a `content` array of `{ type: 'text', text }` items:
   - **If `logs` is non-empty**: first push one text item = `logs.join('\n')`.
   - **If `error`**: push text item `` `Error: ${error.message}` `` and return `{ content, isError: true }`.
   - **Else if `result !== undefined`**: push text item = `JSON.stringify(result, null, 2)` (2-space indent) and return `{ content, isError: false }`.
   - If `result === undefined` and no error, return `{ content, isError: false }` (content may contain only logs, or be empty).

Subtle ordering: logs always come **before** the result/error text item in the content array. Result is serialized with `JSON.stringify(value, null, 2)`.

## 5. Tool Definitions (exact)

Array `toolDefinitions` at `mcp-main.ts:65-313`. Nine tools, in this order. All `inputSchema.type` is `"object"`. JSON Schema types as written (`boolean`, `string`, `number`).

### 5.1 `ListServices`
- description: `List services with structured output`
- properties: `showAll` (boolean, "Show all services or just current directory (optional)")
- required: none
- handler: `handleList({ showAll })` → returns `ListOutput` (`src/list-command.ts:9`):
  ```ts
  { processes: { command, workingDir, uptime, pid, status, serviceName, configChanged? }[], showAll?, message? }
  ```
  `showAll=true` lists all DB processes (no config file needed, status always `'RUNNING'`); otherwise lists processes for the current project dir.

### 5.2 `ListPorts`
- description: `List open ports for running services`
- properties: `showAll` (boolean), `serviceName` (string, "Filter to a specific service name (optional)")
- required: none
- handler: `handleListPorts({ showAll, commandNames: serviceName ? [serviceName] : [] })` → `ListPortsOutput` (`src/list-ports-command.ts:19`):
  ```ts
  { ports: { serviceName, pid, port, address, protocol, isChildProcess }[] }
  ```
  Note: only the single `serviceName` is passed through as a one-element `commandNames` array.

### 5.3 `OpenBrowser`
- description: `Open a browser window to a running service's port` (note literal apostrophe, escaped in source)
- properties: `serviceName` (string, "Name of the service to open in browser")
- required: `["serviceName"]`
- handler: validates `serviceName` present (else `McpError(InvalidRequest, 'Service name is required')`), calls `findProjectDir()`, then `handleOpenBrowser({ projectDir, serviceName })` → `OpenBrowserOutput` `{ serviceName, port, url }` (`src/open-browser-command.ts:12`). Spawns the platform browser opener.

### 5.4 `GetLogs`
- description: `Get recent logs for a specific service`
- properties: `name` (string), `limit` (number, "Maximum number of log lines to return (optional)"), `projectDir` (string, "Project directory where the service is defined (optional - for cross-directory access)")
- required: `["name"]`
- handler: `handleLogsCommand({ commandNames: [name], limit: limit ?? 200, projectDir })`.
  - `DEFAULT_LOGS_LIMIT = 200` (`mcp-main.ts:52`).
  - `handleLogsCommand` returns `void` (`src/logs-command.ts:12`) — it **prints logs to console**, so the actual log output is captured by the `ConsoleLogInterceptor` and surfaced through `callResult.logs`, not through `result`. This is the one tool whose output flows entirely through the log-interception path.

### 5.5 `StartService`
- description: `Start a config-defined service (use StartTransientService for transient processes)`
- properties: `name` (string)
- required: `["name"]`
- handler: validate name; `findProjectDir()`; `startOneService({ projectDir, commandName: name, consoleOutputFormat: 'pretty' })` → `StartResult` `{ projectDir, serviceName }` (`src/start/startOneService.ts:24`).

### 5.6 `StartTransientService`
- description: `Start a transient process with a custom shell command (not defined in config file)`
- properties: `name` (string, "Name for the transient process"), `shell` (string, "Shell command to run the service"), `root` (string, "Root directory for the service (optional, relative to project)")
- required: `["name", "shell"]`
- handler: validate `name && shell` (else `McpError(InvalidRequest, 'Service name and shell command are required')`); `findProjectDir()`; `startOneService({ projectDir, commandName: name, consoleOutputFormat: 'pretty', shell, root })` → `StartResult`.

### 5.7 `KillService`
- description: `Kill a running service`
- properties: `name` (string)
- required: `["name"]`
- handler: validate name; `findProjectDir()`; `handleKillCommand({ projectDir, commandNames: [name] })`. **Returns nothing** (`result` is `undefined`) — response will contain only intercepted logs (if any) with `isError: false`.

### 5.8 `RestartService`
- description: `Restart a running service. If no name provided, restarts all running services in the project.`
- properties: `name` (string, "Name of the service to restart. If not provided, restarts all running services.")
- required: none
- handler: `findProjectDir()`; `handleRestart({ projectDir, commandNames: name ? [name] : [], consoleOutputFormat: 'pretty' })`. Empty `commandNames` ⇒ restart all running services.

### 5.9 `AddServerConfig`
- description: `Add a new server configuration to .candle.json`
- properties: `name` (string), `shell` (string), `root` (string, "Root directory for the service (optional)")
- required: `["name", "shell"]`
- handler: validate `name && shell`; calls `addServerConfig({ name, shell, root })` (`src/addServerConfig.ts:14`), then `console.log(\`Service '${name}' added successfully to .candle.json\`)`.
  - **Subtle duplicate-log bug to replicate-or-fix decision**: `addServerConfig` itself already `console.log`s the identical success message (`addServerConfig.ts:42`), and the handler logs it again (`mcp-main.ts:310`). Under the interceptor this message appears **twice** in `logs`. Note this for the Rust port — decide whether to preserve.
  - `addServerConfig` finds-or-creates `.candle.json` (default filename `DEFAULT_CONFIG_FILENAME = '.candle.json'`), rejects duplicate service names with `Error("Service '<name>' already exists in configuration")`, validates, and writes with `JSON.stringify(config, null, 2)`.

## 6. ConsoleLogInterceptor (`ConsoleLogInterceptor.ts`)

Purpose: handlers were written for CLI use and emit human-readable output via `console.log`/`console.error`. In the MCP server those must not hit stdout (would corrupt the JSON-RPC stream) and instead must be **captured and returned inside the tool response**. The interceptor monkeypatches the global console for the duration of one handler call.

Mechanism (`callWrapped`, `mcp-main.ts:22-50`):
```js
const logWrapper = new ConsoleLogInterceptor();
logWrapper.install();
try { result = await handler(args); }
catch (e) { error = e; }
finally { logWrapper.remove(); }
```

`ConsoleLogInterceptor` behavior:
- `install()`: saves originals, replaces `console.log` and `console.error`. Idempotent (`isInstalled` guard).
- Each arg is stringified: `typeof arg === 'string' ? arg : JSON.stringify(arg)`, joined with a single space.
- `console.log` → pushes the message verbatim.
- `console.error` → pushes `` `[stderr] ${message}` `` (prefix `"[stderr] "`).
- `remove()`: restores originals, clears `isInstalled`.
- `takeLogs()`: returns a **copy** of collected logs and **clears** the internal buffer.

Error normalization in `callWrapped` (`mcp-main.ts:37-43`): on catch, `error` is rebuilt as `{ message: e.message, stack: e.stack, ...e }` (spreads own enumerable props). Only `error.message` is later surfaced in the response text (`Error: <message>`).

**Rust translation note:** Rust has no global mutable `console` to monkeypatch. Reimplement by giving the command handlers an output sink (a `&mut Vec<String>` or a writer trait) instead of patching globals. Capture both an stdout-equivalent stream and an stderr-equivalent stream, prefixing the latter with `[stderr] `. This is the cleaner-and-required design; do not try to hijack real stdout.

## 7. External Dependencies

| npm package | Symbols used | Purpose | Rust replacement |
|---|---|---|---|
| `@modelcontextprotocol/sdk@1.12.1` | `Server` (`/server/index.js`), `StdioServerTransport` (`/server/stdio.js`), `CallToolRequestSchema`, `ListToolsRequestSchema`, `ErrorCode`, `McpError` (`/types.js`) | MCP server, stdio transport, request schemas, error type | `rmcp` (official Rust MCP SDK, crate `rmcp`) or `mcp-sdk-rs`; provides server + stdio transport + tool registration. Serialize tool schemas with `serde_json`. |
| Node `process` / `child_process` / `fs` / `os` / `url` (in dependencies) | stdin close, spawning, package.json read | runtime | std `tokio` (async stdio), `std::process`, `std::fs`, `which`/`open` crate for browser. |

`ErrorCode` values needed: `MethodNotFound` (-32601), `InvalidRequest` (-32600). `McpError` carries a code + message and serializes into the JSON-RPC error object.

## 8. Behaviors easy to get wrong in Rust

1. **stdout purity** — only MCP frames on stdout. The whole interceptor exists for this; preserve it.
2. **stdin-close → exit(0)** — must be wired explicitly; the transport won't do it.
3. **Content ordering** — logs item first, then result/error item. Result serialized as pretty JSON (2-space). Error text is exactly `Error: <message>` (no stack in the visible text).
4. **`undefined` result vs JSON** — `KillService` and `AddServerConfig` return no structured result; their content is logs-only. Don't emit `"null"` or `"undefined"` text; only push a result item when a value exists.
5. **Single-element `commandNames` arrays** — several tools wrap one name into a one-element list; handlers expect arrays.
6. **`findProjectDir()` throws** if no `.candle.json`/`.candle-setup.json` is found walking up from cwd; that throw becomes an `isError: true` response (not a transport error). Config filenames in priority order: `['.candle.json', '.candle-setup.json']`.
7. **`limit` default 200** applied via `?? DEFAULT_LOGS_LIMIT`, so an explicit `0` would be passed through (nullish, not falsy) — preserve nullish-coalescing semantics, not `||`.
8. **Duplicate success log** in `AddServerConfig` (§5.9).
9. **`McpError(MethodNotFound)`** for unknown tool name — this is a protocol-level error response, distinct from handler errors which return `isError: true` content.

## 9. Rust reimplementation notes

Create (in dependency order):

1. **`mcp::console_capture`** — replace `ConsoleLogInterceptor`. A capture sink type `LogCapture { logs: Vec<String> }` with `push_stdout(msg)` and `push_stderr(msg)` (latter prepends `"[stderr] "`), plus `take_logs() -> Vec<String>`. Thread this sink into the command handlers instead of patching globals. (Requires the command handlers to accept an output sink — a cross-cutting refactor of the `list/logs/start/kill/restart/open-browser/addServerConfig` modules.)

2. **`mcp::tools`** — define a `ToolDefinition { name, description, input_schema: serde_json::Value, handler }` registry mirroring `toolDefinitions` (§5), in the same order. Hardcode the nine schemas as `serde_json::json!` literals matching the exact field names/descriptions/required arrays above.

3. **`mcp::call_wrapped`** — equivalent of `callWrapped`: install capture, run handler, catch error → `{ message, stack? }`, return `{ result: Option<Value>, error, logs }`.

4. **`mcp::server`** — `serve_mcp()`:
   - build server identity from `CARGO_PKG_NAME`/`CARGO_PKG_VERSION`, capabilities `{tools:{}}`, instructions string (§3).
   - register `list_tools` (project name/description/schema) and `call_tool` (lookup → `call_wrapped` → build content array per §4, with `MethodNotFound` for unknown tool).
   - wire stdio transport; on stdin EOF, close transport and `std::process::exit(0)`.

5. **CLI dispatch** — route `candle mcp` / `--mcp` to `serve_mcp()`.

Dependency ordering: the command handlers (list, list-ports, open-browser, logs, start/startOneService, kill, restart, addServerConfig) and the config/db layer must already accept an output sink before the MCP layer can be finished; build `console_capture` + handler-sink refactor first, then `call_wrapped`, then `tools`, then `server`, then CLI wiring.