import { LatestExecutionLogFilter } from './log-filters/LatestExecutionLogFilter.ts';
import { consoleLogRow } from './logs.ts';
import { getProcessLogs } from './logs/processLogs.ts';

interface LogsCommandOptions {
  projectDir: string;
  commandNames: string[];
  limit?: number; // Number of log lines to show
}

export async function handleLogsCommand(req: LogsCommandOptions): Promise<void> {
  const { projectDir, limit = 100 } = req;

  // If no names provided, use default
  const namesToShow = req.commandNames.length > 0 ? req.commandNames : [null];
  const isBlendedMode = namesToShow.length > 1;

  // Get logs and filter to only show logs from the most recent process run
  const allLogs = getProcessLogs({
    commandNames: req.commandNames,
    limit,
    projectDir,
  });

  const logFilter = new LatestExecutionLogFilter();
  logFilter.checkLatestLaunchStatus(allLogs);

  const logs = logFilter.filter(allLogs);

  if (logs.length === 0) {
    if (req.commandNames.length === 1) {
      console.log(`No logs found for command '${req.commandNames[0]}' in project '${projectDir}'.`);
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
