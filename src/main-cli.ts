#! /usr/bin/env node

import type { Argv } from 'yargs';
import yargs from 'yargs';
import { hideBin } from 'yargs/helpers';
import { addServerConfig } from './addServerConfig.ts';
import { findProjectDir } from './configFile.ts';
import { maybeRunCleanup } from './database/cleanup.ts';
import { handleClearDatabaseCommand } from './clear-database-command.ts';
import { handleClearLogsCommand } from './clear-logs-command.ts';
import { handleKill } from './kill-command.ts';
import { handleList, printListOutput } from './list-command.ts';
import { handleLogs } from './logs-command.ts';
import { handleRestart } from './restart-command.ts';
import { handleRun, handleStart } from './run-command.ts';
import { handleWaitForLog } from './wait-for-log-command.ts';
import { handleWatch } from './watch-command.ts';
import { serveMCP } from './mcp.ts';

function configureYargs() {
  return yargs(hideBin(process.argv))
    .option('mcp', {
      type: 'boolean',
      describe: 'Enter MCP server mode',
      default: false,
    })
    .command('run [name]', 'Launch process', (yargs: Argv) => {})
    .command('start [name...]', 'Start process(es) in background and exit', (yargs: Argv) => {})
    .command('restart [name]', 'Restart a process service', () => {})
    .command(
      ['kill [name]', 'stop [name]'],
      'Kill processes started in the current working directory',
      (yargs: Argv) => {}
    )
    .command(
      'kill-all',
      'Kill all running processes that are tracked by Candle',
      (yargs: Argv) => {}
    )
    .command(['list', 'ls'], 'List active processes for current directory', (yargs: Argv) => {})
    .command('list-all', 'List all active processes', (yargs: Argv) => {})
    .command('logs [name]', 'Show recent logs for a process', () => {})
    .command('watch [name]', 'Watch live output from a running process', () => {})
    .command('wait-for-log [name]', 'Wait for a specific log message to appear', (yargs: Argv) => {
      yargs
        .option('message', {
          describe: 'The log message to wait for',
          type: 'string',
          demandOption: true,
        })
        .option('timeout', {
          describe: 'Timeout in seconds (default: 30)',
          type: 'number',
          default: 30,
        });
    })
    .command('clear-logs [name]', 'Clear logs for commands in the current directory', () => {})
    .command('erase-database', 'Erase the database stored at ~/.local/state/candle', () => {})
    .command(
      'add-service <name> <shell>',
      'Add a new service to .candle-setup.json',
      (yargs: Argv) => {
        yargs
          .positional('name', {
            describe: 'Name of the service',
            type: 'string',
          })
          .positional('shell', {
            describe: 'Shell command to run the service',
            type: 'string',
          })
          .option('root', {
            describe: 'Root directory for the service',
            type: 'string',
          });
      }
    )
    .demandCommand(0, 'You need to specify a command')
    .help()
    .version();
}

function parseArgs(): {
  command: string;
  commandName: string;
  commandNames: string[];
  mcp: boolean;
  shell?: string;
  root?: string;
  message?: string;
  timeout?: number;
} {
  const argv = configureYargs().parseSync();

  const command = argv._[0] as string;
  const commandName = argv.name as string;
  const commandNames = Array.isArray(argv.name)
    ? (argv.name as string[])
    : argv.name
      ? [argv.name as string]
      : [];
  const mcp = argv.mcp as boolean;
  const shell = argv.shell as string;
  const root = argv.root as string;
  const message = argv.message as string;
  const timeout = argv.timeout as number;

  return { command, commandName, commandNames, mcp, shell, root, message, timeout };
}

export async function main(): Promise<void> {
  // Handle version flag early
  if (process.argv.includes('-v') || process.argv.includes('--version')) {
    const packageJson = await import('../package.json');
    console.log(packageJson.version);
    return;
  }

  const { command, commandName, commandNames, mcp, shell, root, message, timeout } =
    parseArgs();

  // Check if no arguments - print help
  if (process.argv.length === 2) {
    configureYargs().showHelp();
    return;
  }

  if (mcp) {
    // Enter MCP server mode
    await serveMCP();
    return;
  }

  maybeRunCleanup();

  switch (command) {
    case 'run': {
      await handleRun({ commandName, watchLogs: true, consoleOutputFormat: 'pretty' });
      process.exit(0);
      break;
    }

    case 'start': {
      await handleStart({ commandNames, consoleOutputFormat: 'pretty' });
      process.exit(0);
      break;
    }

    case 'list':
    case 'ls': {
      const output = await handleList({});
      printListOutput(output);
      break;
    }
    case 'list-all': {
      const output = await handleList({ showAll: true });
      printListOutput(output);
      break;
    }
    case 'kill':
    case 'stop': {
      await handleKill({ commandName });
      break;
    }
    case 'kill-all': {
      await handleKill({ allGlobalServices: true });
      break;
    }
    case 'restart': {
      await handleRestart({
        commandName,
        consoleOutputFormat: 'pretty',
        watchLogs: false,
      });
      break;
    }

    case 'logs': {
      await handleLogs({ commandName });
      break;
    }

    case 'watch': {
      await handleWatch({ commandName });
      break;
    }

    case 'wait-for-log': {
      const result = await handleWaitForLog({
        commandName: commandName || 'default',
        message,
        timeoutMs: timeout * 1000,
      });

      if (!result.success) {
        process.exit(1);
      }

      break;
    }

    case 'clear-logs': {
      const projectDir = findProjectDir();
      await handleClearLogsCommand({ projectDir, commandName });
      break;
    }

    case 'erase-database':
      await handleClearDatabaseCommand();
      break;

    case 'add-service': {
      try {
        addServerConfig({
          name: commandName,
          shell: shell,
          root: root,
        });
        console.log(`Service '${commandName}' added successfully to .candle.json`);
      } catch (error) {
        console.error(`Error adding service: ${error.message}`);
        process.exit(1);
      }
      break;
    }

    default:
      console.error(`Error: Unrecognized command '${command}'`);
      console.error(
        'Available commands: run, start, list, ls, list-all, stop, kill, kill-all, restart, logs, watch, wait-for-log, clear-logs, erase-database, add-service'
      );
      process.exit(1);
  }
}

export function printError(error: Error) {
  if ((error as any).isUsageError) {
    console.error(error.message);
  } else {
    console.error(error);
  }
}

// Run main function when called as CLI script
main().catch(error => {
  printError(error);
  process.exit(1);
});
