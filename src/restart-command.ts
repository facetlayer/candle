import { findProjectDir } from './configFile.ts';
import { findProcessesByCommandNameAndProjectDir, findProcessesByProjectDir, type ProcessEntry } from './database/processTable.ts';
import { handleKillCommand } from './kill-command.ts';
import { startOneService } from './start/startOneService.ts';

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

  try {
    // If no command names provided, restart all running processes in the project
    if (commandNames.length === 0) {
      const runningProcesses = findProcessesByProjectDir(projectDir);
      commandNames = runningProcesses.map(p => p.command_name);
    }

    // First, fetch process info for all command names before killing
    const processInfoMap = new Map<string, ProcessEntry | undefined>();
    for (const commandName of commandNames) {
      const processes = findProcessesByCommandNameAndProjectDir(commandName, projectDir);
      processInfoMap.set(commandName, processes[0]);
    }

    // Kill all existing processes
    await handleKillCommand({ projectDir, commandNames });

    // Then restart each service using stored shell/root if available
    for (const commandName of commandNames) {
      const runningProcess = processInfoMap.get(commandName);
      await startOneService({
        projectDir,
        commandName,
        consoleOutputFormat,
        shell: runningProcess?.shell,
        root: runningProcess?.root,
      });
    }
  } catch (error) {
    console.error(`Failed to restart: ${error.message}`);
  }
}
