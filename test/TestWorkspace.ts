import * as path from 'path';
import * as fs from 'fs';
import { runShellCommand } from '@facetlayer/subprocess-wrapper';
import { mcpShell, MCPStdinSubprocess } from 'expect-mcp';

export interface CommandResult {
    stdout: string;
    stderr: string;
    code: number;
}

export interface CliOptions {
    cwd?: string;
    env?: Record<string, string>;
}

/**
 * TestWorkspace manages an isolated workspace directory for a test suite.
 *
 * Usage:
 *   const workspace = new TestWorkspace('my-test-suite');
 *   const cli = workspace.createCli();
 *
 *   afterAll(() => workspace.cleanup());
 *
 * The workspace:
 * - Uses a directory under test/workspaces/{name} as both cwd and CANDLE_DATABASE_DIR
 * - Each workspace should have its own .candle.json committed to git
 * - Provides a CLI helper that runs commands in the workspace
 * - Cleans up processes with 'kill-all' on cleanup (but leaves database files)
 */
export class TestWorkspace {
    readonly name: string;
    readonly dbDir: string;
    private static readonly BASE_DIR = path.join(__dirname, 'workspaces');
    private cliPath: string;

    constructor(name: string) {
        this.name = name;
        this.dbDir = path.join(TestWorkspace.BASE_DIR, name);
        this.cliPath = path.join(__dirname, '..', 'src', 'main-cli.ts');

        // Ensure the workspace directory exists
        if (!fs.existsSync(this.dbDir)) {
            fs.mkdirSync(this.dbDir, { recursive: true });
        }
    }

    /**
     * Creates a CLI runner function that automatically sets CANDLE_DATABASE_DIR
     * and uses the workspace directory as the default cwd.
     */
    createCli() {
        const dbDir = this.dbDir;
        const cliPath = this.cliPath;

        return async function runCandleCommand(
            args: string[],
            options: CliOptions = {}
        ): Promise<CommandResult> {
            const env = {
                ...process.env,
                CANDLE_DATABASE_DIR: dbDir,
                // Ensure consistent output for testing
                FORCE_COLOR: '0',
                ...(options.env || {}),
            };

            const result = await runShellCommand('node', [cliPath, ...args], {
                cwd: options.cwd ?? dbDir,
                env,
            });

            return {
                stdout: result.stdoutAsString(),
                stderr: Array.isArray(result.stderr) ? result.stderr.join('\n') : result.stderr || '',
                code: result.exitCode || 0,
            };
        };
    }

    /**
     * Creates an MCP subprocess that connects to candle via stdio.
     * Uses the workspace directory as both cwd and CANDLE_DATABASE_DIR.
     */
    createMcpApp(options: { allowDebugLogging?: boolean } = {}): MCPStdinSubprocess {
        const { allowDebugLogging = false } = options;

        return mcpShell(`node ${this.cliPath} --mcp`, {
            allowDebugLogging,
            cwd: this.dbDir,
            env: {
                ...process.env,
                CANDLE_DATABASE_DIR: this.dbDir,
            },
        });
    }

    /**
     * Cleans up any running processes managed by this workspace.
     * Call this in afterAll() to prevent orphaned processes.
     *
     * Note: This does NOT delete the database directory - just kills processes.
     */
    async cleanup() {
        const env = {
            ...process.env,
            CANDLE_DATABASE_DIR: this.dbDir,
            FORCE_COLOR: '0',
        };

        try {
            await runShellCommand('node', [this.cliPath, 'kill-all'], {
                cwd: this.dbDir,
                env,
            });
        } catch {
            // Ignore errors - processes may already be stopped
        }
    }
}
