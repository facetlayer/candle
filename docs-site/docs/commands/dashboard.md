# dashboard

Run the Candle web dashboard server in the foreground.

## Syntax

```bash
candle dashboard [--port <n>]
```

## Description

Serves the same web dashboard as [`open-dashboard`](./open-dashboard), but in the current terminal instead of as a background service. It prints its URL and runs until you press Ctrl-C. It does not open a browser.

Most of the time you want `candle open-dashboard`, which runs this server in the background for you.

The server listens on `localhost` only, and only answers requests addressed to `localhost`, `127.0.0.1`, or `[::1]`.

## Options

- `--port <n>` - Listen on this port. Without it, the dashboard uses port 7700, or any free port if 7700 is taken.

## Examples

```bash
candle dashboard --port 8000
# Candle dashboard running at http://localhost:8000
# Press Ctrl-C to stop.
```
