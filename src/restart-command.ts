import { getServiceConfigByName } from './configFile.ts';
import { handleKill } from './kill-command.ts';
import { handleRun } from './run-command.ts';

interface RestartOptions {
  commandName: string;
  consoleOutputFormat: 'pretty' | 'json';
  watchLogs: boolean;
}

export async function handleRestart(options: RestartOptions) {
  const { commandName, consoleOutputFormat, watchLogs } = options;

  try {
    // First kill the existing process
    await handleKill({ commandName });

    // Then start it again
    await handleRun({
      commandName,
      consoleOutputFormat,
      watchLogs,
    });

  } catch (error) {
    console.error(`Failed to restart: ${error.message}`);
  }
}
