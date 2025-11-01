# Candle GUI

A desktop GUI application for viewing and managing Candle processes.

## Overview

This is an Electron-based GUI application that provides a visual interface for interacting with Candle, a process manager optimized for local development and AI agents.

## Features

- View all running Candle processes
- See process details (PID, uptime, working directory)
- View real-time logs for each process
- Auto-refresh process list and logs
- Filter between project-specific and all processes

## Architecture

The app uses:
- **Electron** for the desktop application framework
- **Express.js** for the local API server (running in Electron's main process)
- **@facetlayer/candle** library for accessing Candle's process database
- **Spark Framework** (referenced for potential future integration)

## Setup

### Install dependencies

```bash
pnpm install
```

### Build

```bash
pnpm build
```

### Run

```bash
pnpm start
```

## Development

The app runs an Express server inside the Electron main process that serves:
- API endpoints for fetching process data and logs
- Static HTML/CSS/JS files for the UI

The UI connects to `http://localhost:<random-port>` where the Express server is running.

## Project Structure

```
gui/
├── src/
│   ├── main.ts           # Electron main process entry point
│   ├── server.ts         # Express API server
│   └── services/         # Service definitions (for potential Spark integration)
├── public/
│   ├── index.html        # Main UI
│   ├── styles.css        # UI styling
│   └── app.js            # Frontend JavaScript
├── package.json
└── tsconfig.json
```

## Notes

See `SparkFrameworkTODO.md` for details on Spark Framework integration considerations.
