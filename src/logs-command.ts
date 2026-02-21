import { LatestExecutionLogFilter } from './log-filters/LatestExecutionLogFilter.ts';
import { consoleLogRow } from './logs.ts';
import { getProcessLogsWithEvictionInfo } from './logs/processLogs.ts';

interface LogsCommandOptions {
  projectDir: string;
  commandNames: string[];
  limit?: number; // Number of log lines to show
  startAtId?: number; // Only show logs after this log ID
}

export async function handleLogsCommand(req: LogsCommandOptions): Promise<void> {
  const { projectDir, commandNames, limit = 100, startAtId } = req;

  const isBlendedMode = commandNames.length !== 1;

  // Get logs and filter to only show logs from the most recent process run
  const { logs: allLogs, logsWereEvicted } = getProcessLogsWithEvictionInfo({
    projectDir,
    commandNames: commandNames.length > 0 ? commandNames : undefined,
    limit,
    afterLogId: startAtId,
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

  // Show eviction indicator if older logs were removed
  if (logsWereEvicted) {
    console.log('-- older logs have been removed --');
  }

  // Display logs with prefix in blended mode
  for (const log of logs) {
    consoleLogRow(log, { format: 'pretty', enableAppNamePrefix: isBlendedMode });
  }
}
