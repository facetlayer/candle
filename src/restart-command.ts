import { findConfigFile, findServiceByName } from './configFile.ts';
import { findProcessesByCommandNameAndProjectDir, findRunningProcessesByProjectDir, type ProcessEntry } from './database/processTable.ts';
import { UsageError } from './errors.ts';
import { handleKillCommand } from './kill-command.ts';
import { startOneService } from './start/startOneService.ts';

/*
 isServiceDefinedInConfig

 Returns true if the named service has an entry in the project's .candle.json.
 Used so that restart reloads config-defined services from the config file
 (picking up edits to `shell`/`root`) rather than relaunching with the command
 that was captured when the process was first started.
*/
function isServiceDefinedInConfig(commandName: string, projectDir: string): boolean {
  try {
    const { config } = findConfigFile(projectDir);
    return findServiceByName(config, commandName) !== null;
  } catch {
    return false;
  }
}

interface RestartOptions {
  projectDir: string;
  commandNames: string[];
  consoleOutputFormat: 'pretty' | 'json';
}

export async function handleRestart(options: RestartOptions) {
  const { projectDir, consoleOutputFormat } = options;
  let { commandNames } = options;

  if (!projectDir) {
    throw new Error('handleRestart: projectDir is required');
  }

  // If no command names provided, restart all running processes in the project
  if (commandNames.length === 0) {
    const runningProcesses = findRunningProcessesByProjectDir(projectDir);
    if (runningProcesses.length === 0) {
      throw new UsageError('No running processes found in this project to restart');
    }
    // Deduplicate command names to avoid killing the same service multiple times
    commandNames = [...new Set(runningProcesses.map(p => p.command_name))];
  }

  try {
    // First, fetch process info for all command names before killing
    const processInfoMap = new Map<string, ProcessEntry | undefined>();
    for (const commandName of commandNames) {
      const processes = findProcessesByCommandNameAndProjectDir(commandName, projectDir);
      processInfoMap.set(commandName, processes[0]);
    }

    // Kill all existing processes
    await handleKillCommand({ projectDir, commandNames });

    // Then restart each service. For config-defined services, reload the
    // command from .candle.json so edits to `shell`/`root` take effect on
    // restart. Only fall back to the stored shell/root for transient
    // processes that aren't present in the config file.
    for (const commandName of commandNames) {
      let shell: string | undefined;
      let root: string | undefined;

      if (!isServiceDefinedInConfig(commandName, projectDir)) {
        const runningProcess = processInfoMap.get(commandName);
        shell = runningProcess?.shell;
        root = runningProcess?.root;
      }

      await startOneService({
        projectDir,
        commandName,
        consoleOutputFormat,
        shell,
        root,
      });
    }
  } catch (error) {
    console.error(`Failed to restart: ${error.message}`);
  }
}
