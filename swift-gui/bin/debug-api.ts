#!/usr/bin/env node
// CLI for the swift-gui (Candle) debug introspection server.
// Run: ./bin/debug-api.ts <subcommand> [args...]
// Requires Node 23.6+ (built-in TypeScript) or run with `bun bin/debug-api.ts`.

import { writeFileSync } from "node:fs";

const PORT = process.env.CANDLE_DEBUG_PORT ?? "4044";
const BASE = `http://127.0.0.1:${PORT}`;

type Json = unknown;

async function get(path: string): Promise<Response> {
    const res = await fetch(`${BASE}${path}`);
    if (!res.ok && res.headers.get("content-type")?.includes("json")) {
        const body = await res.json();
        throw new Error(`${res.status} ${res.statusText}: ${JSON.stringify(body)}`);
    }
    if (!res.ok) {
        throw new Error(`${res.status} ${res.statusText}`);
    }
    return res;
}

async function getJson(path: string): Promise<Json> {
    return (await get(path)).json();
}

async function postAction(body: Record<string, unknown>): Promise<Json> {
    const res = await fetch(`${BASE}/action`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify(body),
    });
    const json = await res.json();
    if (!res.ok) {
        throw new Error(`${res.status} ${res.statusText}: ${JSON.stringify(json)}`);
    }
    return json;
}

function pretty(v: Json): string {
    return JSON.stringify(v, null, 2);
}

interface ServiceState {
    serviceName: string;
    projectDir: string;
    pid: number;
    uptime: string;
    status: string;
    isRunning: boolean;
}

interface State {
    loading: boolean;
    errorMessage: string | null;
    autoScroll: boolean;
    actionInProgress: string | null;
    selected: { serviceName: string; projectDir: string } | null;
    services: ServiceState[];
    groups: Array<{ projectDir: string; displayName: string; services: ServiceState[] }>;
    logCount: number;
    lastLogId: number | null;
    logs: Array<{ id: number; timestamp: number; log_type: number; command_name: string; content: string }>;
}

const COMMANDS: Record<string, {
    describe: string;
    usage?: string;
    run: (args: string[]) => Promise<void>;
}> = {
    state: {
        describe: "Print the current AppStore snapshot.",
        run: async () => {
            console.log(pretty(await getJson("/state")));
        },
    },

    services: {
        describe: "List services (project, name, pid, status).",
        run: async () => {
            const s = (await getJson("/state")) as State;
            for (const g of s.groups) {
                for (const svc of g.services) {
                    console.log(`${g.displayName}\t${svc.serviceName}\t${svc.pid}\t${svc.status}\t${svc.uptime}`);
                }
            }
        },
    },

    select: {
        describe: "Select a service (loads logs). Pass --clear to deselect.",
        usage: "select <service-name> <project-dir>   |   select --clear",
        run: async (args) => {
            if (args[0] === "--clear") {
                await postAction({ type: "deselect" });
                console.log("ok");
                return;
            }
            const [name, projectDir] = args;
            if (!name || !projectDir) throw new Error("usage: select <name> <project-dir>");
            const r = (await postAction({ type: "selectService", name, projectDir })) as { state: State };
            console.log(pretty(r.state.selected));
        },
    },

    refresh: {
        describe: "Refresh services list.",
        run: async () => {
            await postAction({ type: "refresh" });
            console.log("ok");
        },
    },

    "refresh-logs": {
        describe: "Refresh logs for the selected service.",
        run: async () => {
            const r = (await postAction({ type: "refreshLogs" })) as { state: State };
            console.log(`logs: ${r.state.logCount} (last id: ${r.state.lastLogId ?? "-"})`);
        },
    },

    logs: {
        describe: "Print the last N logs for the selected service.",
        usage: "logs [N]",
        run: async (args) => {
            const n = Number(args[0] ?? "20") || 20;
            const s = (await getJson("/state")) as State;
            const tail = s.logs.slice(-n);
            for (const e of tail) {
                const t = new Date(e.timestamp * 1000).toISOString().slice(11, 19);
                console.log(`${t}\t#${e.id}\t[${e.log_type}]\t${e.content.replace(/\n$/, "")}`);
            }
        },
    },

    "auto-scroll": {
        describe: "Set auto-scroll on/off.",
        usage: "auto-scroll <on|off>",
        run: async (args) => {
            const v = args[0];
            if (v !== "on" && v !== "off") throw new Error("expected 'on' or 'off'");
            await postAction({ type: "setAutoScroll", value: v === "on" });
            console.log("ok");
        },
    },

    start: {
        describe: "Start a service.",
        usage: "start <name> <project-dir>",
        run: async (args) => {
            const [name, projectDir] = args;
            if (!name || !projectDir) throw new Error("usage: start <name> <project-dir>");
            await postAction({ type: "start", name, projectDir });
            console.log("ok");
        },
    },

    restart: {
        describe: "Restart a service.",
        usage: "restart <name> <project-dir>",
        run: async (args) => {
            const [name, projectDir] = args;
            if (!name || !projectDir) throw new Error("usage: restart <name> <project-dir>");
            await postAction({ type: "restart", name, projectDir });
            console.log("ok");
        },
    },

    kill: {
        describe: "Kill a service.",
        usage: "kill <name> <project-dir>",
        run: async (args) => {
            const [name, projectDir] = args;
            if (!name || !projectDir) throw new Error("usage: kill <name> <project-dir>");
            await postAction({ type: "kill", name, projectDir });
            console.log("ok");
        },
    },

    "open-browser": {
        describe: "Open the selected service's URL in a browser.",
        usage: "open-browser <name> <project-dir>",
        run: async (args) => {
            const [name, projectDir] = args;
            if (!name || !projectDir) throw new Error("usage: open-browser <name> <project-dir>");
            await postAction({ type: "openInBrowser", name, projectDir });
            console.log("ok");
        },
    },

    "dismiss-error": {
        describe: "Dismiss the error banner.",
        run: async () => {
            await postAction({ type: "dismissError" });
            console.log("ok");
        },
    },

    screen: {
        describe: "Save a PNG of the current window.",
        usage: "screen [out.png]   (default: /tmp/swift-gui-screen.png)",
        run: async (args) => {
            const out = args[0] ?? "/tmp/swift-gui-screen.png";
            const res = await get("/screen");
            const buf = Buffer.from(await res.arrayBuffer());
            writeFileSync(out, buf);
            console.log(`${out}\t${buf.length} bytes`);
        },
    },

    wait: {
        describe: "Block until the debug server is reachable.",
        usage: "wait [timeout-seconds]   (default: 30)",
        run: async (args) => {
            const timeoutMs = (Number(args[0] ?? "30") || 30) * 1000;
            const start = Date.now();
            while (Date.now() - start < timeoutMs) {
                try {
                    await get("/state");
                    console.log("ready");
                    return;
                } catch {
                    await new Promise((r) => setTimeout(r, 200));
                }
            }
            throw new Error(`timeout after ${timeoutMs}ms`);
        },
    },

    ping: {
        describe: "One-shot reachability check; exits 0 if up, 1 if not.",
        run: async () => {
            try {
                await get("/state");
                console.log("up");
            } catch (e) {
                console.error(`down: ${(e as Error).message}`);
                process.exit(1);
            }
        },
    },

    raw: {
        describe: "Send a raw POST /action JSON body.",
        usage: 'raw \'{"type":"...", ...}\'',
        run: async (args) => {
            const body = args[0];
            if (!body) throw new Error("missing JSON body");
            const r = await postAction(JSON.parse(body));
            console.log(pretty(r));
        },
    },

    help: {
        describe: "Show this help.",
        run: async () => printHelp(),
    },
};

function printHelp(): void {
    console.log(`debug-api — CLI for the swift-gui debug server (${BASE})`);
    console.log("");
    console.log("Subcommands:");
    const names = Object.keys(COMMANDS).sort();
    const width = Math.max(...names.map((n) => (COMMANDS[n].usage ?? n).length));
    for (const n of names) {
        const u = COMMANDS[n].usage ?? n;
        console.log(`  ${u.padEnd(width)}  ${COMMANDS[n].describe}`);
    }
    console.log("");
    console.log("Env: CANDLE_DEBUG_PORT (default 4044)");
}

async function main(): Promise<void> {
    const [cmd, ...rest] = process.argv.slice(2);
    if (!cmd || cmd === "-h" || cmd === "--help") {
        printHelp();
        return;
    }
    const handler = COMMANDS[cmd];
    if (!handler) {
        console.error(`unknown subcommand: ${cmd}`);
        printHelp();
        process.exit(2);
    }
    try {
        await handler.run(rest);
    } catch (e) {
        console.error(`error: ${(e as Error).message}`);
        process.exit(1);
    }
}

main();
