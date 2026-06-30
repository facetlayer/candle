---
name: rust-log-collector
description: Guide to the experimental Rust-based log collector
---

# Rust Log Collector (Experimental)

Candle includes an experimental log collector written in Rust as an alternative to the default Node.js-based collector. The Rust collector can offer lower overhead and faster log throughput for high-volume services.

**Status:** Experimental. The Rust collector is functionally equivalent to the Node.js collector but has not been as widely tested in production use.

> **Note:** This page describes the **published Node.js CLI**, where `logCollector` chooses between the Node and Rust *collector sidecars*. The separate Rust port of the Candle CLI itself (under `rust/`) always uses the Rust collector and ignores this setting — see the [Rust port plan](../rust/PORTING_PLAN.md).

## How It Works

When you start a service with `candle start`, Candle spawns a separate log collector process that captures stdout/stderr from the service and writes the output to the SQLite database. By default, this collector is a Node.js process. The Rust collector is a compiled binary that does the same job.

## Building the Rust Collector

Before enabling the Rust collector, you must build the binary:

```bash
cd rust
cargo build --release
```

This produces the binary at `rust/target/release/candle-log-collector`.

## Enabling the Rust Collector

### Option 1: Using `set-config`

The simplest way to enable the Rust collector is with the `set-config` command:

```bash
# Enable the Rust log collector
candle set-config logCollector rust

# Switch back to the default Node.js collector
candle set-config logCollector node
```

### Option 2: Editing `.candle.json` directly

Add the `logCollector` field to your project's `.candle.json`:

```json
{
  "services": [
    { "name": "api", "shell": "npm run dev" }
  ],
  "logCollector": "rust"
}
```

Valid values are `"node"` (default) and `"rust"`.

## Verifying the Collector

After enabling the Rust collector, restart your services for the change to take effect:

```bash
candle restart my-service
```

If the Rust binary has not been built, Candle will report an error when starting a service and provide build instructions.

## When to Use

- **High-volume log output:** The Rust collector uses less CPU when processing large amounts of stdout/stderr.
- **Resource-constrained environments:** Lower memory footprint compared to spawning an additional Node.js process per service.

For most development workflows, the default Node.js collector works well and requires no extra setup.
