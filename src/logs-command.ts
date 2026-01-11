import { LatestExecutionLogFilter } from './log-filters/LatestExecutionLogFilter.ts';
import { consoleLogRow } from './logs.ts';
import { getProcessLogs } from './logs/processLogs.ts';

interface LogsCommandOptions {
  projectDir: string;
  commandNames: string[];
  limit?: number; // Number of log lines to show
}

export async function handleLogsCommand(req: LogsCommandOptions): Promise<void> {
  const { projectDir, commandNames, limit = 100 } = req;

  const isBlendedMode = commandNames.length !== 1;

  // Get logs and filter to only show logs from the most recent process run
  const allLogs = getProcessLogs({
    projectDir,
    commandNames: commandNames.length > 0 ? commandNames : undefined,
    limit,
  });

  const logFilter = new LatestExecutionLogFilter({ showPastLogsBehavior: 'show_logs_from_previous_launch' });
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
    consoleLogRow(log, { format: 'pretty', enableAppNamePrefix: isBlendedMode });
  }
}
