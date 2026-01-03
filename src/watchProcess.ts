import { AfterProcessStartLogFilter } from './afterProcessStartFilter.ts';
import { consoleLogRow, consoleLogSystemMessage } from './logs.ts';
import { LogIterator } from './logs/LogIterator.ts';
import { ProcessLogType, type ProcessLog } from './logs/processLogs.ts';

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

  // Use filter to only show logs after process_start_initiated for each command
  // This ensures we see logs from the current run of each process
  const logFilter = new AfterProcessStartLogFilter();

  const logIterator = new LogIterator({
    projectDir,
    commandNames,
  });
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
  const runningProcesses = new Set(commandNames);

  function printLogs(logs: ProcessLog[]): void {
    const filteredLogs = logFilter.filter(logs);

    filteredLogs.forEach(log => {
      const prefix = isBlendedMode ? `[${log.command_name}] ` : undefined;
      consoleLogRow(log, { format: consoleOutputFormat, prefix });
      if (log.log_type === ProcessLogType.process_exited) {
        runningProcesses.delete(log.command_name);
      }
    });
  }

  // Initial log fetch
  printLogs(logIterator.getNextLogs({ limit: INITIAL_LOG_COUNT }));

  while (watching && runningProcesses.size > 0) {
    printLogs(logIterator.getNextLogs({}));

    // Wait before next poll
    await new Promise(resolve => setTimeout(resolve, POLL_INTERVAL));
  }

  // Clean up timeout if it exists
  if (timeoutId) {
    clearTimeout(timeoutId);
  }

  if (runningProcesses.size > 0) {
    if (runningProcesses.size === 1) {
      consoleLogSystemMessage(
        consoleOutputFormat,
        'Stopped watching. Process is still running in background.'
      );
    } else {
      consoleLogSystemMessage(
        consoleOutputFormat,
        `Stopped watching. ${runningProcesses.size} processes are still running in background.`
      );
    }
  }

  process.removeListener('SIGINT', stopWatching);
  process.removeListener('SIGTERM', stopWatching);
}
