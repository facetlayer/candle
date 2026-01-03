import { LatestExecutionLogFilter } from './log-filters/LatestExecutionLogFilter.ts';
import { getServiceInfoByName } from './configFile.ts';
import { consoleLogRow } from './logs.ts';
import { getProcessLogs } from './logs/processLogs.ts';

interface LogsCommandOptions {
  commandNames: string[];
  limit?: number; // Number of log lines to show
  projectDir?: string; // Optional project directory for cross-directory access
}

export async function handleLogs(options: LogsCommandOptions): Promise<void> {
  const { limit = 100 } = options;

  // If no names provided, use default
  const namesToShow = options.commandNames.length > 0 ? options.commandNames : [null];
  const isBlendedMode = namesToShow.length > 1;

  // Resolve all command names to their actual names and project directories
  const resolvedServices: { commandName: string; projectDir: string }[] = [];

  for (const name of namesToShow) {
    if (options.projectDir) {
      // Use the provided project directory directly
      resolvedServices.push({
        projectDir: options.projectDir,
        commandName: name,
      });
    } else {
      // Get service info - works for both config-defined and transient processes
      const info = getServiceInfoByName(name);
      resolvedServices.push({
        projectDir: info.projectDir,
        commandName: info.commandName,
      });
    }
  }

  // All services should be in the same project directory
  const projectDir = resolvedServices[0].projectDir;
  const commandNames = resolvedServices.map(s => s.commandName);

  // Get logs and filter to only show logs from the most recent process run
  const allLogs = getProcessLogs({
    commandNames,
    limit,
    projectDir,
  });

  const logFilter = new LatestExecutionLogFilter();
  logFilter.checkLatestLaunchStatus(allLogs);

  const logs = logFilter.filter(allLogs);

  if (logs.length === 0) {
    if (commandNames.length === 1) {
      console.log(`No logs found for command '${commandNames[0]}' in project '${projectDir}'.`);
    } else {
      console.log(`No logs found for commands in project '${projectDir}'.`);
    }
    return;
  }

  // Display logs with prefix in blended mode
  for (const log of logs) {
    const prefix = isBlendedMode ? `[${log.command_name}] ` : undefined;
    consoleLogRow(log, { format: 'pretty', prefix });
  }
}
