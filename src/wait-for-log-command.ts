import { LatestExecutionLogFilter } from './log-filters/LatestExecutionLogFilter.ts';
import { getServiceInfoByName } from './configFile.ts';
import { consoleLogRow } from './logs.ts';
import { LogIterator } from './logs/LogIterator.ts';
import { ProcessLogType } from './logs/ProcessLogType.ts';
import { getProcessLogs } from './logs/processLogs.ts';

const POLL_INTERVAL = 200;
const LOG_COUNT_SEARCH_LIMIT = 1000;

interface WaitForLogOptions {
  projectDir: string;
  commandNames: string[];
  message: string;
  timeoutMs?: number;
}

function printRecentLogs(projectDir: string, commandNames: string[]) {
  console.log(`Recent logs for '${commandNames.join(', ')}':`);
  const filter = new LatestExecutionLogFilter();
  const allLogs = getProcessLogs({
    commandNames,
    limit: 100,
    projectDir,
  });
  const recentLogs = filter.filter(allLogs);
  for (const log of recentLogs) {
    consoleLogRow(log, { format: 'pretty' });
  }
}

export async function handleWaitForLog(options: WaitForLogOptions) {
  const { projectDir, commandNames, message, timeoutMs = 30000 } = options;

  // Get recent logs
  const logIterator = new LogIterator({
    projectDir,
    commandNames,
    limit: LOG_COUNT_SEARCH_LIMIT,
  });
  const allInitialLogs = logIterator.getNextLogs();

  // Use filter to only show logs from the most recent process run
  const logFilter = new LatestExecutionLogFilter();
  logFilter.checkLatestLaunchStatus(allInitialLogs);
  const initialLogs = logFilter.filter(allInitialLogs);

  // Check if we have any logs at all for this process
  if (initialLogs.length === 0) {
    return {
      success: false,
      message: 'Process has not started yet',
    };
  }

  // Check if we have any process_has_started events
  const hasProcessStarted = initialLogs.some(
    log => log.log_type === ProcessLogType.process_start_initiated
  );

  if (!hasProcessStarted) {
    console.error(`Process has not started yet`);
    return {
      success: false,
    };
  }

  // Look for the message in existing logs
  for (const logEvent of initialLogs) {
    if (logEvent.content?.includes(message)) {
      console.log(`Found message "${message}" in existing logs.`);
      return {
        success: true,
      };
    }
  }

  // Poll for logs until we find the message or timeout
  let timeStarted = Date.now();
  while (true) {
    if (Date.now() - timeStarted > timeoutMs) {
      console.log(`wait-for-log failed: Timed out after ${timeoutMs}ms and message "${message}" not found.`);
      printRecentLogs(projectDir, commandNames);
      return {
        success: false,
      };
    }

    const rawLogs = logIterator.getNextLogs();
    const logs = logFilter.filter(rawLogs);
    for (const log of logs) {
      if (log.content?.includes(message)) {
        console.log(`Found message "${message}" in logs.`);
        return {
          success: true,
        };
      }

      if (log.log_type === ProcessLogType.process_exited) {
        console.log(`wait-for-log failed: Process exited before finding message "${message}"`);
        printRecentLogs(projectDir, commandNames);
        return {
          success: false,
        };
      }
    }

    await new Promise(resolve => setTimeout(resolve, POLL_INTERVAL));
  }
}
