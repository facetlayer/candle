import { getServiceInfoByName } from './configFile.ts';
import { consoleLogRow } from './logs.ts';
import { LogIterator } from './logs/LogIterator.ts';
import { getProcessLogs, ProcessLogType } from './logs/processLogs.ts';

const POLL_INTERVAL = 200;
const LOG_COUNT_SEARCH_LIMIT = 1000;

interface WaitForLogOptions {
  commandName: string;
  message: string;
  timeoutMs?: number;
}

function printRecentLogs(commandName: string, projectDir: string) {
  console.log(`Recent logs for '${commandName}':`);
  const recentLogs = getProcessLogs({
    commandNames: [commandName],
    limit: 100,
    limitToLatestProcessLogs: true,
    projectDir,
  });
  for (const log of recentLogs) {
    consoleLogRow('pretty', log);
  }
}

export async function handleWaitForLog(options: WaitForLogOptions) {
  const { message, timeoutMs = 30000 } = options;

  // Get service info - works for both config-defined and transient processes
  const { commandName, projectDir } = getServiceInfoByName(options.commandName);

  // Get recent logs
  const logIterator = new LogIterator({
    projectDir,
    commandNames: [commandName],
    limit: LOG_COUNT_SEARCH_LIMIT,
    limitToLatestProcessLogs: true,
  });
  const initialLogs = logIterator.getNextLogs();

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
      printRecentLogs(commandName, projectDir);
      return {
        success: false,
      };
    }

    const logs = logIterator.getNextLogs();
    for (const log of logs) {
      if (log.content?.includes(message)) {
        console.log(`Found message "${message}" in logs.`);
        return {
          success: true,
        };
      }

      if (log.log_type === ProcessLogType.process_exited) {
        console.log(`wait-for-log failed: Process exited before finding message "${message}"`);
        printRecentLogs(commandName, projectDir);
        return {
          success: false,
        };
      }
    }

    await new Promise(resolve => setTimeout(resolve, POLL_INTERVAL));
  }
}
