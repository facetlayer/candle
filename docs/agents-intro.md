---
name: agents-intro
description: Short introduction to the Candle tool for AI agents
---

# Candle

Candle is a CLI tool for launching and managing background processes. It's
designed for local development, such as running localhost servers
or other long-running tasks.

An important concept is that all Candle processes are organized by project
directory, and each project is separate. (The project directory is the location
of the nearest `.candle.json` file).

# Basic commands:

 - See what services are available: `candle ls`
 - Start a service in the background: `candle run <service-name>`
 - Browse logs: `candle logs` or `candle logs <service-name>`
 - Configure a new service: `candle add-service <service-name> --shell <shell>`
 - Kill a service: `candle kill <service-name>`
 - Restart a service: `candle restart <service-name>`
 - Kill all services in this project: `candle kill`

More commands are available by running: `candle help`

Candle can also help with localhost port reservations, see: `candle help port-reservation`

# Commands to avoid

Do NOT use these commands: `candle run` or `candle watch`. These are interactive commands
and they will block your execution until killed. Use other commands like `candle start`
or `candle logs` instead.
