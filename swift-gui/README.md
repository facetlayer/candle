# swift-gui

Native macOS SwiftUI port of `../gui` — a process manager for Candle.

Patterned after `~/tools/cc-skills-gui-swift`: SPM executable target, SwiftUI
`@main` app with a single `WindowGroup`, `ObservableObject` store, and a
`scripts/build-app.sh` that wraps the binary into a `.app` bundle.

Unlike the Electron-based original, this app talks to the Candle GUI API
(`gui/src/services/candle-service.ts`) over HTTP rather than embedding a web
view.

## Layout

```
Package.swift               SPM manifest, macOS 13+
Sources/SwiftGui/
  App.swift                 @main + ContentView + header
  AppStore.swift            @MainActor ObservableObject, polling tasks
  Models.swift              ServiceProcess, LogEntry, LogType, grouping
  CandleApi.swift           URLSession HTTP client
  SidebarView.swift         project-grouped service list with action buttons
  LogView.swift             auto-scrolling log pane
  Theme.swift               dark color palette
scripts/build-app.sh        produces build/Candle.app
test/smoke-test.sh          build, launch, screenshot, teardown
```

## Build & run

```bash
# 1. Make sure the Candle GUI API is running
cd ../gui && candle start api

# 2. Build the .app
cd ../swift-gui
./scripts/build-app.sh           # release (default)
./scripts/build-app.sh debug

# 3. Launch
open build/Candle.app
```

The default API URL is `http://localhost:4022/api` (matching `gui/.env`).
Override with the `CANDLE_API_URL` env var:

```bash
CANDLE_API_URL=http://localhost:4800/api open build/Candle.app
```

## Endpoints used

All proxied through Prism's `/api` mount:

| Method | Path                          | Purpose                       |
|--------|-------------------------------|-------------------------------|
| GET    | `/services`                   | list all candle processes     |
| GET    | `/services/:name/logs`        | poll logs (afterLogId, limit) |
| POST   | `/services/:name/start`       | start service                 |
| POST   | `/services/:name/restart`     | restart service               |
| POST   | `/services/:name/kill`        | kill service                  |
| GET    | `/services/:name/url`         | resolve a service's port URL  |

Polling intervals match the React app: services every 2s, logs every 1s.

## Testing

The app embeds a loopback HTTP introspection server (pattern lifted from
`~/tools/facetlayer-desktop`, see `docs/introspection.md` there). It binds
`127.0.0.1:4044` only and lets test scripts read state, drive actions, and
capture window pixels — no AppleScript, no screen takeover.

### Enable / disable

| Env var               | Default                  | Effect                                       |
|-----------------------|--------------------------|----------------------------------------------|
| `CANDLE_DEBUG_SERVER` | `1` in DEBUG, `0` release| Set to `1` in release builds to enable       |
| `CANDLE_DEBUG_PORT`   | `4044`                   | Override the listen port                     |

### Endpoints

```
GET  /              endpoint listing
GET  /state         JSON snapshot of AppStore (services, selected, logs, …)
GET  /screen        PNG of the current main window
POST /action        drive the UI (JSON body: {"type": "...", ...})
```

Action types: `selectService`, `deselect`, `refresh`, `refreshLogs`,
`setAutoScroll`, `start`, `restart`, `kill`, `openInBrowser`, `dismissError`.

### CLI: `bin/debug-api.ts`

Thin Node CLI wrapping the HTTP API. Requires Node 23.6+ for built-in TS.

```bash
./bin/debug-api.ts help
./bin/debug-api.ts wait              # block until server is ready
./bin/debug-api.ts services          # tabular list
./bin/debug-api.ts select api /Users/andy/candle/gui
./bin/debug-api.ts logs 20
./bin/debug-api.ts auto-scroll off
./bin/debug-api.ts screen out.png    # capture window pixels
./bin/debug-api.ts raw '{"type":"refresh"}'
```

### Test scripts

```bash
./test/debug-server-test.sh    # primary: drives the UI via the debug server
./test/smoke-test.sh           # legacy: build/launch/screencapture only
```
