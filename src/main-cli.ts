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
import { handleGetReservedPort, printGetReservedPortOutput } from './get-reserved-port-command.ts';
import { handleKillCommand } from './kill-command.ts';
import { handleKillAll } from './kill-all-command.ts';
import { handleList, printListOutput } from './list-command.ts';
import { handleListPorts, printListPortsOutput } from './list-ports-command.ts';
import { handleListReservedPorts, printListReservedPortsOutput } from './list-reserved-ports-command.ts';
import { handleLogsCommand } from './logs-command.ts';
import { handleReleasePorts, printReleasePortsOutput } from './release-ports-command.ts';
import { handleReservePort, printReservePortOutput } from './reserve-port-command.ts';
import { handleRestart } from './restart-command.ts';
import { handleRunCommand } from './run-command.ts';
import { handleStartCommand } from './start-command.ts';
import { handleWaitForLog } from './wait-for-log-command.ts';
import { handleWatch } from './watch-command.ts';
import { serveMCP } from './mcp/mcp-main.ts';
import { assertValidCommandNames } from './cli/assertValidCommandName.ts';

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);
const __packageRoot = join(__dirname, '..');

const docFiles = new DocFilesHelper({
  dirs: [join(__packageRoot, 'docs')],
  files: [join(__packageRoot, 'README.md')],
});

function printGroupedHelp() {
  console.log(`Usage: candle <command> [options]

Process Management:
  list, ls                  List processes for this project directory
  run [name...]             Launch process(es) and watch their output
  run                       Launch all configured processes and watch their output
  start [name...]           Start process(es) in background
  start                     Start all configured processes in the background
  restart [name]            Restart a running process
  restart                   Restart all processes for this project directory
  kill [name...]            Kill running process(es)
  kill                      Kill all process(es) in this project directory
  list-ports                List currently active ports for running services
                            (Note: This is different than port reservations)

Logs:
  logs [name...]            Show recent logs for process(es)
  watch [name...]           Watch live output from process(es)
  wait-for-log [name]       Wait for a specific log message

Configuration:
  add-service <name>        Add a new service to .candle.json

Port Reservation:
  list-reserved-ports       List reserved ports for this current project directory
  reserve-port [name]       Reserve a port for a service
  release-ports [name]      Release reserved port(s)
  release-ports             Release all port reservations for this project directory
  get-reserved-port <name>  Get the reserved port for a service
      
Documentation:
  list-docs                 List available documentation
  get-doc <name>            Display a documentation file

Troubleshooting & Maintenance:
  list-all                  List all managed processes on this system
  kill-all                  Kill all managed processes on this system
  list-ports-all            List currently active ports for all managed processes
  list-reserved-ports-all   List reserved ports for all projects
  clear-logs [name]         Clear logs for process(es)
  erase-database            Erase the Candle database

Options:
  --mcp                     Enter MCP server mode
  --help                    Show help
  --version                 Show version number

Run 'candle <command> --help' for more information on a command.`);
}

function configureYargs() {
  return yargs(hideBin(process.argv))
    .scriptName('candle')
    .usage('Usage: $0 <command> [options]')
    .option('mcp', {
      type: 'boolean',
      describe: 'Enter MCP server mode',
      default: false,
    })

    // Process Management
    .command('run [name...]', 'Launch process(es) and watch their output', (yargs: Argv) => {
      yargs
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
        });
    })
    .command('start [name...]', 'Start process(es) in background and exit', (yargs: Argv) => {
      yargs
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
        });
    })
    .command('restart [name]', 'Restart a running process', () => {})
    .command('kill [name...]', 'Kill process(es) in the current directory', () => {})
    .command('kill-all', 'Kill all running processes', () => {})
    .command(['list', 'ls'], 'List processes for current directory', () => {})
    .command('list-all', 'List all processes', () => {})
    .command('logs [name...]', 'Show recent logs for process(es)', () => {})
    .command('watch [name...]', 'Watch live output from process(es)', () => {})
    .command('wait-for-log [name]', 'Wait for a specific log message', (yargs: Argv) => {
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
    .command('list-ports', 'List open ports for running services', () => {})
    .command('list-ports-all', 'List open ports for all services', () => {})

    // Port Reservations
    .command('reserve-port [name]', 'Reserve an unused port', () => {})
    .command('release-ports [name]', 'Release reserved port(s)', () => {})
    .command('list-reserved-ports', 'List reserved ports for project', () => {})
    .command('list-reserved-ports-all', 'List all reserved ports', () => {})
    .command('get-reserved-port <name>', 'Get the reserved port for a service', () => {})

    // Configuration & Maintenance
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
        });
    })
    .command('clear-logs [name]', 'Clear logs for process(es)', () => {})
    .command('erase-database', 'Erase the Candle database', () => {})
    .command('list-docs', 'List available documentation', () => {})
    .command('get-doc <name>', 'Display a documentation file', () => {})

    .demandCommand(0, 'You need to specify a command')
    .help(false)  // Disable default help, we'll handle it
    .version();
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

  return { command, commandNames, mcp, shell, root, enableStdin, message, timeout };
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

  const { command, commandNames, mcp, shell, root, enableStdin, message, timeout } =
    parseArgs();

  // Check if no arguments - print help
  if (process.argv.length === 2) {
    printGroupedHelp();
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
      const output = await handleListPorts({});
      printListPortsOutput(output);
      break;
    }

    case 'list-ports-all': {
      const output = await handleListPorts({ showAll: true });
      printListPortsOutput(output);
      break;
    }

    case 'reserve-port': {
      const projectDir = findProjectDir();
      const serviceName = commandNames[0];
      const output = await handleReservePort({ projectDir, serviceName });
      printReservePortOutput(output);
      break;
    }

    case 'release-ports': {
      const projectDir = findProjectDir();
      const serviceName = commandNames[0];
      const output = await handleReleasePorts({ projectDir, serviceName });
      printReleasePortsOutput(output);
      break;
    }

    case 'list-reserved-ports': {
      const output = await handleListReservedPorts({});
      printListReservedPortsOutput(output);
      break;
    }

    case 'list-reserved-ports-all': {
      const output = await handleListReservedPorts({ showAll: true });
      printListReservedPortsOutput(output);
      break;
    }

    case 'get-reserved-port': {
      const projectDir = findProjectDir();
      const serviceName = commandNames[0];
      if (!serviceName) {
        console.error('Error: Service name is required');
        process.exit(1);
      }
      const output = await handleGetReservedPort({ projectDir, serviceName });
      printGetReservedPortOutput(output);
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
      });
      break;
    }

    case 'watch': {
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

    case 'add-service': {

      const commandName = commandNames[0];
      if (!commandName) {
        console.error('Error: No command name provided');
        process.exit(1);
      }

      if (commandNames.length > 1) {
        console.error('Error: Cannot use multiple command names for add-service');
        process.exit(1);
      }

      try {
        addServerConfig({
          name: commandName,
          shell: shell,
          root: root,
          enableStdin: enableStdin,
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
        'Available commands: run, start, list, ls, list-all, list-ports, list-ports-all, reserve-port, release-ports, list-reserved-ports, list-reserved-ports-all, get-reserved-port, kill, kill-all, restart, logs, watch, wait-for-log, clear-logs, erase-database, add-service, list-docs, get-doc'
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
