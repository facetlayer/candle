import { ExecutionStatusTracker } from './log-filters/ExecutionStatusTracker.ts';
import { LatestExecutionLogFilter } from './log-filters/LatestExecutionLogFilter.ts';
import { consoleLogRow, consoleLogSystemMessage } from './logs.ts';
import { LogIterator } from './logs/LogIterator.ts';
import { type ProcessLog } from './logs/processLogs.ts';
import { ProcessLogType } from './logs/ProcessLogType.ts';

const INITIAL_LOG_COUNT = 100;
const POLL_INTERVAL = 200;

interface WatchOptions {
  projectDir: string;
  commandNames: string[];
  exitAfterMs?: number; // Optional timeout to exit watching after a certain period
  consoleOutputFormat: 'pretty' | 'json';
}

export async function watchProcess(options: WatchOptions): Promise<void> {
  const { projectDir, commandNames, exitAfterMs, consoleOutputFormat } = options;
  const isBlendedMode = commandNames.length > 1;

  const logIterator = new LogIterator({
    projectDir,
    commandNames,
  });

  // Use filter to only show logs from the most recent process launch for each command
  const logFilter = new LatestExecutionLogFilter();
  const initialLogs = logIterator.getNextLogs({ limit: INITIAL_LOG_COUNT });
  logFilter.checkLatestLaunchStatus(initialLogs);

  let watching = true;

  // Set up timeout if exitAfterMs is provided
  let timeoutId: NodeJS.Timeout | null = null;
  if (exitAfterMs && exitAfterMs > 0) {
    timeoutId = setTimeout(() => {
      consoleLogSystemMessage(
        consoleOutputFormat,
        `Exiting watch mode after ${exitAfterMs}ms timeout`
      );
      watching = false;
    }, exitAfterMs);
  }

  // Handle signals to stop watching
  const stopWatching = () => {
    watching = false;
    if (timeoutId) {
      clearTimeout(timeoutId);
    }
  };

  process.on('SIGINT', stopWatching);
  process.on('SIGTERM', stopWatching);

  // Track which processes are still running
  const executionStatusTracker = new ExecutionStatusTracker();

  function printLogs(logs: ProcessLog[]): void {
    executionStatusTracker.apply(logs);
    const filteredLogs = logFilter.filter(logs);

    filteredLogs.forEach(log => {
      const prefix = isBlendedMode ? `[${log.command_name}] ` : undefined;
      consoleLogRow(log, { format: consoleOutputFormat, prefix });

    });
  }

  // Print initial logs (already fetched for status check)
  printLogs(initialLogs);

  while (watching) {
    printLogs(logIterator.getNextLogs({}));

    // Wait before next poll
    await new Promise(resolve => setTimeout(resolve, POLL_INTERVAL));
  }

  // Clean up timeout if it exists
  if (timeoutId) {
    clearTimeout(timeoutId);
  }

  const runningProcesses = executionStatusTracker.countRunningProcesses();
  if (runningProcesses > 0) {
    if (runningProcesses === 1) {
      consoleLogSystemMessage(
        consoleOutputFormat,
        'Stopped watching. Process is still running in the background.'
      );
    } else {
      consoleLogSystemMessage(
        consoleOutputFormat,
        `Stopped watching. ${runningProcesses} processes are still running in the background.`
      );
    }
  }

  process.removeListener('SIGINT', stopWatching);
  process.removeListener('SIGTERM', stopWatching);
}
