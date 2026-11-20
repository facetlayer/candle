# Dashboard

`candle dashboard run` serves a localhost web UI listing every running service; `candle dashboard
open` runs that server as a Candle-managed background service and `candle dashboard close` stops it. Code: `rust/src/dashboard/`.

## Global namespace

Every process row is keyed by `project_dir`, and `start` normally requires a `.candle.json`. Candle's
own services (only the dashboard today) use a reserved project key instead:
`dirs::global_project_dir()` = `<state dir>/global`. It lives under the state dir so it follows
`CANDLE_DATABASE_DIR` in tests, is a real directory (created on demand) usable as the service's cwd,
and never coincides with a user project. `find-orphans` exempts it (`dirs::is_global_project_dir`).

`dashboard open` (`dashboard/service.rs`) launches through the normal `start_one_service` path as a transient (`--shell`)
service named `dashboard`, with shell `<current_exe> dashboard run [--port N]`. So it gets a monitor,
logs, `list-all`/`kill-all` coverage, and the start lock for free. A running dashboard is reused
unless it came from a different binary path (e.g. after an upgrade) or a different `--port` was
asked for, in which case it is replaced. After starting, the command polls `list-ports` for the
service's port (the server may fall back from 7700 to a free port) and opens it via
`open_browser::open_service_port`. `dashboard close` kills that service with the regular
`handle_kill_command`.

## Server

`dashboard/http.rs` is a minimal HTTP/1.1 implementation on `std::net`: one request per connection,
`Content-Length` bodies only (chunked is rejected), size limits on lines/headers/bodies, and
`Connection: close` on every response. A thread per connection; each request opens its own SQLite
connection. It binds `127.0.0.1` only.

Routes (`dashboard/mod.rs`):

| Route | |
|---|---|
| `GET /`, `/app.js`, `/style.css` | Embedded UI from `rust/assets/dashboard/` (`include_str!`) |
| `GET /api/processes` | `{version, globalProjectDir, processes: [...]}`: live, unkilled rows system-wide with ports (deduped by port number), sorted by project then name |
| `POST /api/open-browser` | `{projectDir, serviceName, port?}` → `open_service_port`. The port must belong to that service |
| `POST /api/kill` | `{projectDir, serviceName, pid?}` → `handle_kill_command`. 404 if not running; 409 if `pid` no longer matches the running instance (it restarted); 400 for the dashboard's own service, since killing it would drop the request |

Request hardening: every request must carry a loopback `Host` (`localhost`, `127.0.0.1`, `[::1]`)
to defeat DNS rebinding, and the POSTs require `Content-Type: application/json` so cross-origin
pages cannot send it without a CORS preflight (which is never approved).

## UI build

The React source is in `../dashboard` (its own pnpm project; esbuild, no TypeScript). `pnpm build`
writes `index.html`, `style.css`, and a minified `app.js` to `rust/assets/dashboard/`. The output is
committed because release builds run `cargo build --locked` without Node.
