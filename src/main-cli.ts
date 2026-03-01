#! /usr/bin/env node

import type { Argv } from 'yargs';
import yargs from 'yargs';
import { hideBin } from 'yargs/helpers';
import { dirname, join } from 'path';
import { fileURLToPath } from 'url';
import { readFileSync } from 'fs';
import { DocFilesHelper } from '@facetlayer/doc-files-helper';
import { addServerConfig } from './addServerConfig.ts';
import { findProjectDir } from './configFile.ts';
import { maybeRunCleanup } from './database/cleanup.ts';
import { handleClearDatabaseCommand } from './clear-database-command.ts';
import { handleClearLogsCommand } from './clear-logs-command.ts';
import { handleOpenBrowser, printOpenBrowserOutput } from './open-browser-command.ts';
import { handleKillCommand } from './kill-command.ts';
import { handleKillAll } from './kill-all-command.ts';
import { handleList, printListOutput } from './list-command.ts';
import { handleListPorts, printListPortsOutput } from './list-ports-command.ts';
import { handleLogsCommand } from './logs-command.ts';
import { handleRestart } from './restart-command.ts';
import { handleRunCommand } from './run-command.ts';
import { handleStartCommand } from './start-command.ts';
import { handleWaitForLog } from './wait-for-log-command.ts';
import { handleSetupProject } from './setup-project-command.ts';
import { handleWatch } from './watch-command.ts';
import { serveMCP } from './mcp/mcp-main.ts';
import { assertValidCommandNames } from './cli/assertValidCommandName.ts';
import { isRunByAgent } from './runContext.ts';

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);
const __packageRoot = join(__dirname, '..');

const docFiles = new DocFilesHelper({
  dirs: [join(__packageRoot, 'docs')],
  files: [join(__packageRoot, 'README.md')],
});

function printGroupedHelp() {
  const runLines = isRunByAgent ? '' : `
  run [names...]            Launch process(es) and watch their output`;
  const watchLines = isRunByAgent ? '' : `
  watch [name...]           Watch live output from process(es)`;

  console.log(`Usage: candle <command> [options]

Process Management:
  list, ls                  List processes for this project directory${runLines}
  start [names...]          Start process(es) in background
  check-start [names...]    Start process(es) only if not already running
  restart [names...]        Restart running process(es)
  kill [names...]           Kill running process(es)

Port Detection:
  list-ports                Uses the OS to detect and list the active open ports
  open-browser [name]       Open browser to service (auto-detects if one running)

Logs:
  logs [name...]            Show recent logs for process(es)${watchLines}
  wait-for-log [name]       Wait for a specific log message

Configuration:
  setup-project             Create a new .candle.json in the current directory
  add-service [name] ...    Add a new service to .candle.json

Documentation:
  list-docs                 List available documentation
  get-doc <name>            Display a documentation file

Troubleshooting & Maintenance:
  list-all                  List all managed processes on this system
  kill-all                  Kill all managed processes on this system
  list-ports-all            List currently active ports for all managed processes
  clear-logs [name]         Clear logs for process(es)
  erase-database            Erase the Candle database

Options:
  help                      Show help
  mcp                       Enter MCP server mode
  --version                 Show version number

Run 'candle <command> --help' for more information on a command.`);
}

function configureYargs() {
  return yargs(hideBin(process.argv))
    .scriptName('candle')
    .usage('Usage: $0 <command> [options]')
    .option('mcp', {
      type: 'boolean',
      hidden: true,
      default: false,
    })

    // Help and MCP commands
    .command('help [topic]', 'Show help', (yargs: Argv) => { yargs.positional('topic', { type: 'string' }).strictOptions(); })
    .command('mcp', 'Enter MCP server mode', (yargs: Argv) => { yargs.strictOptions(); })

    // Process Management
    .command('run [name...]', 'Launch process(es) and watch their output', (yargs: Argv) => {
      yargs
        .positional('name', { type: 'string' })
        .option('shell', {
          describe: 'Shell command for transient process',
          type: 'string',
        })
        .option('root', {
          describe: 'Root directory for transient process',
          type: 'string',
        })
        .option('enable-stdin', {
          describe: 'Enable stdin message polling from database',
          type: 'boolean',
        })
        .strictOptions();
    })
    .command('start [name...]', 'Start process(es) in background and exit', (yargs: Argv) => {
      yargs
        .positional('name', { type: 'string' })
        .option('shell', {
          describe: 'Shell command for transient process',
          type: 'string',
        })
        .option('root', {
          describe: 'Root directory for transient process',
          type: 'string',
        })
        .option('enable-stdin', {
          describe: 'Enable stdin message polling from database',
          type: 'boolean',
        })
        .strictOptions();
    })
    .command('check-start [name...]', 'Start process(es) only if not already running', (yargs: Argv) => {
      yargs
        .positional('name', { type: 'string' })
        .option('shell', {
          describe: 'Shell command for transient process',
          type: 'string',
        })
        .option('root', {
          describe: 'Root directory for transient process',
          type: 'string',
        })
        .option('enable-stdin', {
          describe: 'Enable stdin message polling from database',
          type: 'boolean',
        })
        .strictOptions();
    })
    .command('restart [name]', 'Restart a running process', (yargs: Argv) => { yargs.positional('name', { type: 'string' }).strictOptions(); })
    .command('kill [name...]', 'Kill process(es) in the current directory', (yargs: Argv) => { yargs.positional('name', { type: 'string' }).strictOptions(); })
    .command('kill-all', 'Kill all running processes', (yargs: Argv) => { yargs.strictOptions(); })
    .command(['list', 'ls'], 'List processes for current directory', (yargs: Argv) => { yargs.strictOptions(); })
    .command('list-all', 'List all processes', (yargs: Argv) => { yargs.strictOptions(); })
    .command('logs [name...]', 'Show recent logs for process(es)', (yargs: Argv) => {
      yargs
        .positional('name', { type: 'string' })
        .option('count', {
          describe: 'Number of log lines to show (default: 100)',
          type: 'number',
        })
        .option('start-at', {
          describe: 'Only show logs after this log ID',
          type: 'number',
        })
        .strictOptions();
    })
    .command('watch [name...]', 'Watch live output from process(es)', (yargs: Argv) => { yargs.positional('name', { type: 'string' }).strictOptions(); })
    .command('wait-for-log [name]', 'Wait for a specific log message', (yargs: Argv) => {
      yargs
        .positional('name', { type: 'string' })
        .option('message', {
          describe: 'The log message to wait for',
          type: 'string',
          demandOption: true,
        })
        .option('timeout', {
          describe: 'Timeout in seconds (default: 30)',
          type: 'number',
          default: 30,
        })
        .strictOptions();
    })
    .command('list-ports [name]', 'List open ports for running services', (yargs: Argv) => { yargs.positional('name', { type: 'string' }).strictOptions(); })
    .command('list-ports-all', 'List open ports for all services', (yargs: Argv) => { yargs.strictOptions(); })
    .command('open-browser [name]', 'Open browser to a running service (auto-detects if only one running)', (yargs: Argv) => { yargs.positional('name', { type: 'string' }).strictOptions(); })

    // Configuration & Maintenance
    .command('setup-project', 'Create a new .candle.json in the current directory', (yargs: Argv) => { yargs.strictOptions(); })
    .command('add-service <name>', 'Add a new service to .candle.json', (yargs: Argv) => {
      yargs
        .positional('name', {
          describe: 'Name of the service',
          type: 'string',
        })
        .option('shell', {
          describe: 'Shell command to run the service',
          type: 'string',
          demandOption: true,
        })
        .option('root', {
          describe: 'Root directory for the service',
          type: 'string',
        })
        .option('enable-stdin', {
          describe: 'Enable stdin message polling from database',
          type: 'boolean',
        })
        .strictOptions();
    })
    .command('clear-logs [name]', 'Clear logs for process(es)', (yargs: Argv) => { yargs.positional('name', { type: 'string' }).strictOptions(); })
    .command('erase-database', 'Erase the Candle database', (yargs: Argv) => { yargs.strictOptions(); })
    .command('list-docs', 'List available documentation', (yargs: Argv) => { yargs.strictOptions(); })
    .command('get-doc <name>', 'Display a documentation file', (yargs: Argv) => { yargs.positional('name', { type: 'string' }).strictOptions(); })

    .demandCommand(0, 'You need to specify a command')
    .help(false)  // Disable default help, we'll handle it
    .version();
}

function requireServiceName(commandNames: string[]): string {
  const serviceName = commandNames[0];
  if (!serviceName) {
    console.error('Error: Service name is required');
    process.exit(1);
  }
  return serviceName;
}

function parseArgs(): {
  command: string;
  commandNames: string[];
  mcp: boolean;
  shell?: string;
  root?: string;
  enableStdin?: boolean;
  message?: string;
  timeout?: number;
  topic?: string;
  count?: number;
  startAt?: number;
} {
  const argv = configureYargs().parseSync();

  const command = argv._[0] as string;
  const commandNames = Array.isArray(argv.name)
    ? (argv.name as string[])
    : argv.name
      ? [argv.name as string]
      : [];
  const mcp = argv.mcp as boolean;
  const shell = argv.shell as string;
  const root = argv.root as string;
  const enableStdin = argv['enable-stdin'] as boolean;
  const message = argv.message as string;
  const timeout = argv.timeout as number;
  const topic = argv.topic as string;
  const count = argv.count as number;
  const startAt = argv['start-at'] as number;

  return { command, commandNames, mcp, shell, root, enableStdin, message, timeout, topic, count, startAt };
}

export async function main(): Promise<void> {
  // Handle version flag early
  if (process.argv.includes('-v') || process.argv.includes('--version')) {
    const packageJsonPath = join(dirname(fileURLToPath(import.meta.url)), '..', 'package.json');
    const packageJson = JSON.parse(readFileSync(packageJsonPath, 'utf-8'));
    console.log(packageJson.version);
    return;
  }

  // Handle help flag - show grouped help
  if (process.argv.includes('--help') || process.argv.includes('-h')) {
    // If help is for a specific command, let yargs handle it
    const hasCommand = process.argv.some(arg =>
      !arg.startsWith('-') && arg !== process.argv[0] && arg !== process.argv[1]
    );
    if (hasCommand) {
      // Let yargs show command-specific help
      configureYargs().help(true).parse();
      return;
    }
    printGroupedHelp();
    return;
  }

  const { command, commandNames, mcp, shell, root, enableStdin, message, timeout, topic, count, startAt } =
    parseArgs();

  // Check if no arguments - print help
  if (process.argv.length === 2) {
    printGroupedHelp();
    return;
  }

  // Handle 'mcp' command or --mcp flag
  if (command === 'mcp' || mcp) {
    await serveMCP();
    return;
  }

  // Handle 'help' command
  if (command === 'help') {
    if (topic) {
      console.error(`Unknown help topic: ${topic}`);
      process.exit(1);
    } else {
      printGroupedHelp();
    }
    return;
  }

  maybeRunCleanup();

  switch (command) {
    case 'run': {
      if (isRunByAgent) {
        console.error("Error: 'run' is not available in agent mode. Use 'candle start' to start processes and 'candle logs' to view their output.");
        process.exit(1);
      }
      const projectDir = findProjectDir();
      await handleRunCommand({ projectDir, commandNames, shell, root, enableStdin });
      break;
    }

    case 'start': {
      const projectDir = findProjectDir();
      await handleStartCommand({ projectDir, commandNames, consoleOutputFormat: 'pretty', shell, root, enableStdin });
      process.exit(0);
      break;
    }

    case 'check-start': {
      const projectDir = findProjectDir();
      await handleStartCommand({ projectDir, commandNames, consoleOutputFormat: 'pretty', shell, root, enableStdin, checkStart: true });
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

    case 'list-ports': {
      const serviceName = commandNames[0];
      const output = await handleListPorts({ serviceName });
      printListPortsOutput(output);
      break;
    }

    case 'list-ports-all': {
      const output = await handleListPorts({ showAll: true });
      printListPortsOutput(output);
      break;
    }

    case 'open-browser': {
      const serviceName = commandNames[0];
      const output = await handleOpenBrowser({ serviceName });
      printOpenBrowserOutput(output);
      break;
    }

    case 'kill': {
      const projectDir = findProjectDir();
      assertValidCommandNames(commandNames);
      await handleKillCommand({ projectDir, commandNames });
      break;
    }

    case 'kill-all': {
      await handleKillAll();
      break;
    }

    case 'restart': {
      const projectDir = findProjectDir();
      assertValidCommandNames(commandNames);
      await handleRestart({
        projectDir,
        commandNames,
        consoleOutputFormat: 'pretty',
      });
      process.exit(0);
      break;
    }

    case 'logs': {
      const projectDir = findProjectDir();
      // Don't validate command names - allow viewing logs for transient processes
      await handleLogsCommand({
        projectDir,
        commandNames,
        limit: count,
        startAtId: startAt,
      });
      break;
    }

    case 'watch': {
      if (isRunByAgent) {
        console.error("Error: 'watch' is not available in agent mode. Use 'candle logs' to view process output.");
        process.exit(1);
      }
      // Don't validate command names - allow watching transient processes
      await handleWatch({ commandNames });
      break;
    }

    case 'wait-for-log': {
      const projectDir = findProjectDir();
      // Don't validate command names - allow waiting for logs from transient processes
      const result = await handleWaitForLog({
        projectDir,
        commandNames,
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
      // Don't validate command names - allow clearing logs for dead transient processes
      await handleClearLogsCommand({ projectDir, commandNames });
      break;
    }

    case 'erase-database':
      await handleClearDatabaseCommand();
      break;

    case 'setup-project': {
      handleSetupProject();
      break;
    }

    case 'add-service': {
      const commandName = requireServiceName(commandNames);
      if (commandNames.length > 1) {
        console.error('Error: Cannot use multiple command names for add-service');
        process.exit(1);
      }
      try {
        addServerConfig({
          name: commandName,
          shell,
          root,
          enableStdin,
        });
      } catch (error) {
        console.error(`Error adding service: ${error.message}`);
        process.exit(1);
      }
      break;
    }

    case 'list-docs':
      docFiles.printDocFileList();
      break;

    case 'get-doc':
      docFiles.printDocFileContents(commandNames[0]);
      break;

    default:
      console.error(`Error: Unrecognized command '${command}'`);
      console.error(
        'Run "candle help" for available commands.'
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
