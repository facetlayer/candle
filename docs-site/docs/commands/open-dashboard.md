# open-dashboard

Open the Candle web dashboard, starting it in the background if needed.

## Syntax

```bash
candle open-dashboard [--port <n>] [--no-browser]
```

## Description

The dashboard is a web page that lists every service Candle is running on this machine, grouped by project, with each service's PID, uptime, command, and listening ports. Each port has an **Open** button that opens it in your browser, the same way [`open-browser`](./open-browser) does. The page refreshes every two seconds.

`open-dashboard` runs the dashboard as a background service that Candle manages, so it keeps running after the command exits. If the dashboard is already running, the command reuses it; otherwise it starts it, waits for it to listen, and opens it in your browser.

You can run `open-dashboard` from any directory. The dashboard service belongs to no project: it runs in Candle's global namespace, under the state directory. It shows up as `dashboard` in [`list-all`](./list-all), is never reported by [`find-orphans`](./find-orphans), and stops with [`kill-all`](./kill-all).

The dashboard listens on `localhost` only, on port 7700 by default (or any free port if 7700 is taken).

## Options

- `--port <n>` - Listen on this port. A dashboard already running on a different port is restarted on this one.
- `--no-browser` - Print the dashboard's URL without opening a browser.

## Examples

```bash
candle open-dashboard
# Started the Candle dashboard at http://localhost:7700 (pid 41235)
# Opened http://localhost:7700 in browser
```

```bash
candle open-dashboard --no-browser
# The Candle dashboard is running at http://localhost:7700 (pid 41235)
```

## See also

- [`dashboard`](./dashboard) - Run the dashboard server in the foreground
- [`list-all`](./list-all) - List all managed processes from the terminal
