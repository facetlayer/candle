import { getServiceInfoByName } from './configFile.ts';
import { consoleLogRow } from './logs.ts';
import { getProcessLogs } from './logs/processLogs.ts';

interface LogsCommandOptions {
  commandName: string;
  limit?: number; // Number of log lines to show
  projectDir?: string; // Optional project directory for cross-directory access
}

export async function handleLogs(options: LogsCommandOptions): Promise<void> {
  const { limit = 100 } = options;

  let projectDir: string;
  let commandName: string;

  if (options.projectDir) {
    // Use the provided project directory directly
    projectDir = options.projectDir;
    commandName = options.commandName;
  } else {
    // Get service info - works for both config-defined and transient processes
    const info = getServiceInfoByName(options.commandName);
    projectDir = info.projectDir;
    commandName = info.commandName;
  }

  // Get logs using the command name and project directory
  const logs = getProcessLogs({
    commandName,
    limit,
    limitToLatestProcessLogs: true,
    projectDir,
  });

  if (logs.length === 0) {
    console.log(`No logs found for command '${commandName}' in project '${projectDir}'.`);
    return;
  }

  // Display logs
  for (const log of logs) {
    consoleLogRow('pretty', log);
  }
}
