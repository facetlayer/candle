import { getServiceConfigByName } from './configFile.ts';
import { consoleLogRow } from './logs.ts';
import { LogIterator } from './logs/LogIterator.ts';
import { getProcessLogs, ProcessLogType } from './logs/processLogs.ts';

const POLL_INTERVAL = 200;
const LOG_COUNT_SEARCH_LIMIT = 1000;

interface WaitForLogOptions {
  commandName: string;
  message: string;
  timeoutMs?: number; // Default to 30000ms (30 seconds)
}

export async function handleWaitForLog(options: WaitForLogOptions) {
  const { message, timeoutMs = 30000 } = options;
  const { projectDir, serviceConfig } = getServiceConfigByName(options.commandName);

  // Get recent logs
  const logIterator = new LogIterator({
    projectDir,
    commandName: serviceConfig.name,
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
    return {
      success: false,
      message: 'Process has not started yet',
    };
  }

  // Look for the message in existing logs
  for (const logEvent of initialLogs) {
    if (logEvent.content?.includes(message)) {
      return {
        success: true,
        message: `Found message "${message}" in existing logs.`,
      };
    }
  }

  // Poll for logs until we find the message or timeout
  let timeStarted = Date.now();
  while (true) {
    if (Date.now() - timeStarted > timeoutMs) {
      // Print recent logs to help debug the timeout
      const recentLogs = getProcessLogs({
        commandName: serviceConfig.name,
        limit: 100,
        limitToLatestProcessLogs: true,
        projectDir,
      });

      console.error(`\nRecent logs for '${serviceConfig.name}':`);
      for (const log of recentLogs) {
        consoleLogRow('pretty', log);
      }

      return {
        success: false,
        message: `Timeout: Message "${message}" not found within ${timeoutMs}ms`,
      };
    }

    const logs = logIterator.getNextLogs();
    for (const log of logs) {
      if (log.content?.includes(message)) {
        return {
          success: true,
          message: `Found message "${message}" in logs.`,
        };
      }

      if (log.log_type === ProcessLogType.process_exited) {
        return {
          success: false,
          message: `Process exited before finding message "${message}"`,
        };
      }
    }

    await new Promise(resolve => setTimeout(resolve, POLL_INTERVAL));
  }
}
