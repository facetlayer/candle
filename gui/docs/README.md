# Candle GUI

A web-based graphical interface for managing Candle processes. Shows all running
processes across the system, grouped by project directory, with controls to
start, restart, and kill services and view their logs in real time.

## Architecture

The GUI is a two-process setup:

- **API server** (port 4800) - A Prism Framework API that wraps Candle's
  programmatic functions (`findAllProcesses`, `getProcessLogs`,
  `handleStartCommand`, etc.)
- **Web frontend** (port 4801) - A React + Vite app. The Vite dev server
  proxies `/services` requests to the API.

## Running

From the `gui/` directory:

```
candle start api
candle start web
```

Then open http://localhost:4801

## API Endpoints

| Method | Path | Description |
|--------|------|-------------|
| GET | `/services` | List all running processes across the system |
| GET | `/services/:name/logs` | Get logs for a service (query: `projectDir`, `afterLogId`, `limit`) |
| POST | `/services/:name/start` | Start a service (body: `projectDir`) |
| POST | `/services/:name/restart` | Restart a service (body: `projectDir`) |
| POST | `/services/:name/kill` | Kill a service (body: `projectDir`) |

All action and log endpoints require `projectDir` because the same service name
can exist in different projects.

## Implementation Details

### Client/Server Communication

The frontend uses polling to keep the UI up to date. There are no WebSocket or
SSE connections.

**Service list polling:**
- The `ServiceList` component polls `GET /services` every **2 seconds**.
- Each response contains the full list of all running processes across the
  system. The frontend groups them by `projectDir` for display.
- After a user action (start/restart/kill), an immediate refresh is triggered
  on top of the regular polling interval.

**Log polling:**
- The `LogViewer` component polls `GET /services/:name/logs` every **1 second**
  when a service is selected.
- The first request fetches the most recent 200 log entries.
- Subsequent requests pass `afterLogId` (the ID of the last received log entry)
  so only new logs are returned. This is an incremental poll - each response
  contains only the logs that appeared since the last request.
- The frontend accumulates log entries in memory, capping at 2000 entries
  (oldest are discarded) to prevent memory bloat.
- An auto-scroll feature keeps the log view pinned to the bottom as new
  entries arrive. This can be toggled off by the user.

**When the selected service changes**, the log state is fully reset: accumulated
logs are cleared, the `afterLogId` cursor resets to 0, and fresh logs are
fetched from scratch.

### System-Wide Process View

The `GET /services` endpoint calls Candle's `findAllProcesses()` which queries
the shared SQLite process database. This returns every running process managed
by Candle on the machine, regardless of which project directory they belong to.

The frontend groups these by `projectDir` and renders them as collapsible
sections. Each project header shows the last two path segments (e.g.,
`andy/candle`) with the full path available as a tooltip.

### Internal Candle Imports

Some Candle functions needed by the API (`handleStartCommand`,
`handleKillCommand`, `handleRestart`, `formatUptime`) are not part of Candle's
public `index.ts` exports. The GUI imports these via relative paths to the
parent source tree (e.g., `../../../src/start-command.ts`). This works because
the GUI lives inside the Candle repository and Node runs the TypeScript source
directly.
