import { findProjectDir } from './configFile.ts';
import { findProcessesByCommandNameAndProjectDir } from './database/processTable.ts';
import { handleKill } from './kill-command.ts';
import { startOneService } from './start-command.ts';

interface RestartOptions {
  commandName: string;
  consoleOutputFormat: 'pretty' | 'json';
}

export async function handleRestart(options: RestartOptions) {
  const { commandName, consoleOutputFormat } = options;

  try {
    // Find the project directory
    const projectDir = findProjectDir();

    // Look up the running process to get its shell and root from DB
    const processes = findProcessesByCommandNameAndProjectDir(commandName, projectDir);
    const runningProcess = processes[0];

    // Get shell and root from DB if process is running, otherwise will fall back to config
    const shell = runningProcess?.shell;
    const root = runningProcess?.root;

    // First kill the existing process
    await handleKill({ commandNames: [commandName] });

    // Then start it again using stored shell/root if available
    await startOneService({
      commandName,
      consoleOutputFormat,
      shell,
      root,
    });
  } catch (error) {
    console.error(`Failed to restart: ${error.message}`);
  }
}
