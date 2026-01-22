import * as path from 'path';
import * as fs from 'fs';
import { runShellCommand, SubprocessResult } from '@facetlayer/subprocess-wrapper';
import { mcpShell, MCPStdinSubprocess } from 'expect-mcp';

// DEPRECATED: CommandResult is the old response object. This will be deleted. Use SubprocessResult instead.
export interface CommandResult {
    stdout: string;
    stderr: string;
    code: number;
}

export interface CliOptions {
    cwd?: string;
    env?: Record<string, string>;
    ignoreExitCode?: boolean;
}

/**
 * TestWorkspace manages an isolated workspace directory for a test suite.
 *
 * Usage:
 *   const workspace = new TestWorkspace('my-test-suite');
 *   const result = await workspace.runCli(['start', 'echo']);
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

    /*
      Runs a CLI command in the workspace.

      This will use the workspace directory as the current working directory for the shell command,
      so Candle will use the local .candle.json config.

      It also sets the CANDLE_DATABASE_DIR environment variable to the workspace directory, so that
      the command uses the workspace database.
    */
    async runCli(args: string[], options: CliOptions = {}): Promise<SubprocessResult> {
        const cwd = this.dbDir;
        const env = {
            ...process.env,
            CANDLE_DATABASE_DIR: cwd,
            FORCE_COLOR: '0',
            ...(options.env || {}),
        };

        //console.log('runCandleCommand', args, options);

        const result = await runShellCommand('node', [this.cliPath, ...args], {
            cwd: options.cwd ?? cwd,
            env,
        });

        if (result.failed() && !options.ignoreExitCode) {
            throw result.asError();
        }

        return result;
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
     * Ensures a subdirectory exists within the workspace.
     * Useful for tests that use the --root parameter.
     */
    ensureSubdir(name: string): void {
        const subdir = path.join(this.dbDir, name);
        if (!fs.existsSync(subdir)) {
            fs.mkdirSync(subdir, { recursive: true });
        }
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
