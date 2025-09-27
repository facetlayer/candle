import { getServiceConfigByName } from './configFile.ts';
import { handleKill } from './handleKill.ts';
import { handleRun } from './handleRun.ts';

interface RestartOptions {
  commandName: string;
  consoleOutputFormat: 'pretty' | 'json';
  watchLogs: boolean;
}

export async function handleRestart(options: RestartOptions) {
  const { commandName, consoleOutputFormat, watchLogs } = options;
  const { projectDir } = getServiceConfigByName(commandName);

  try {
    // First kill the existing process
    await handleKill({ commandName });

    // Then start it again
    await handleRun({
      commandName,
      consoleOutputFormat,
      watchLogs,
    });

    console.log(`Service '${commandName}' restarted successfully`);
  } catch (error) {
    console.error(`Failed to restart: ${error.message}`);
  }
}
