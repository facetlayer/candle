import { getServiceConfigByName } from './configFile.ts';
import { consoleLogRow } from './logs.ts';
import { getProcessLogs } from './logs/processLogs.ts';

interface LogsCommandOptions {
  commandName: string;
  limit?: number; // Number of log lines to show
}

export async function handleLogs(options: LogsCommandOptions): Promise<void> {
  const { serviceConfig, projectDir } = getServiceConfigByName(options.commandName);
  const { limit = 100 } = options;

  // Get logs using the command name and project directory
  const logs = getProcessLogs({
    commandName: serviceConfig.name,
    limit,
    limitToLatestProcessLogs: true,
    projectDir,
  });

  if (logs.length === 0) {
    console.log(`No logs found for command '${serviceConfig.name}' in project '${projectDir}'.`);
    return;
  }

  // Display logs
  for (const log of logs) {
    consoleLogRow('pretty', log);
  }
}
