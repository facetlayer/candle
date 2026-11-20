# dashboard

Open, close, or run the Candle web dashboard.

## Syntax

```bash
candle dashboard open [--port <n>] [--no-browser]
candle dashboard close
candle dashboard run [--port <n>]
```

## Description

The dashboard is a web page that lists every service Candle is running on this machine, grouped by project, with each service's PID, uptime, command, and listening ports. Each port has an **Open** button that opens it in your browser, the same way [`open-browser`](./open-browser) does. The page refreshes every two seconds.

The dashboard listens on `localhost` only, on port 7700 by default (or any free port if 7700 is taken), and only answers requests addressed to `localhost`, `127.0.0.1`, or `[::1]`.

## Subcommands

### open

Starts the dashboard as a background service that Candle manages, so it keeps running after the command exits, then opens it in your browser. If the dashboard is already running, `open` reuses it.

You can run `dashboard open` from any directory. The dashboard service belongs to no project: it runs in Candle's global namespace, under the state directory. It shows up as `dashboard` in [`list-all`](./list-all), is never reported by [`find-orphans`](./find-orphans), and also stops with [`kill-all`](./kill-all).

### close

Stops the background dashboard started by `dashboard open`.

### run

Runs the dashboard server in the current terminal instead of in the background. It prints its URL and runs until you press Ctrl-C. It does not open a browser.

## Options

- `--port <n>` - Listen on this port (`open`, `run`). With `open`, a dashboard already running on a different port is restarted on this one.
- `--no-browser` - Print the dashboard's URL without opening a browser (`open`).

## Examples

```bash
candle dashboard open
# Started the Candle dashboard at http://localhost:7700 (pid 41235)
# Opened http://localhost:7700 in browser

candle dashboard open --no-browser
# The Candle dashboard is running at http://localhost:7700 (pid 41235)

candle dashboard close
# Stopped the Candle dashboard (pid 41235)

candle dashboard run --port 8000
# Candle dashboard running at http://localhost:8000
# Press Ctrl-C to stop.
```

## See also

- [`list-all`](./list-all) - List all managed processes from the terminal
- [`open-browser`](./open-browser) - Open one service's port from the terminal
