import { type ProcessLog } from '../logs/processLogs.ts';
import { ProcessLogType } from '../logs/ProcessLogType.ts';

interface LaunchStatus {
  startLogId: number;
}

/**
 * AfterProcessStartLogFilter
 *
 * Filters logs to only show logs from the most recent process launch for each command.
 * This ensures that when a process has been run multiple times, only the latest run's
 * logs are shown.
 *
 * Usage:
 * 1. First call checkLatestLaunchStatus(logs) with the existing recent logs.
 * 2. Call filter(logs) to get only logs from the most recent launch
 */
export class LatestExecutionLogFilter {
  recentCommandLaunch = new Map<string, LaunchStatus>();

  /**
   * Analyze logs to determine the latest launch status for each command.
   * Logs should be in chronological order (oldest first).
   *
   * This method scans logs to find the most recent launch status for each command.
   */
  checkLatestLaunchStatus(logs: ProcessLog[]): void {
    this.recentCommandLaunch.clear();

    for (const log of logs) {
      const commandName = log.command_name;

      if (log.log_type === ProcessLogType.process_start_initiated) {
        this.recentCommandLaunch.set(commandName, {
          startLogId: log.id,
        });
      }
    }
  }

  /**
   * Filter logs to only include logs from the most recent launch for each command.
   * Only includes logs with id >= the startLogId determined by checkLatestLaunchStatus.
   */
  filter(logs: ProcessLog[]): ProcessLog[] {
    const result: ProcessLog[] = [];

    for (const log of logs) {
      const commandName = log.command_name;
      const status = this.recentCommandLaunch.get(commandName);

      let shouldIncludeLog = false;

      if (status) {
        if (log.id >= status.startLogId) {
          shouldIncludeLog = true;
        }
      }

      if (!status && log.log_type === ProcessLogType.process_start_initiated) {
        // Mark that we found the start event for this command
        this.recentCommandLaunch.set(commandName, {
          startLogId: log.id,
        });
        shouldIncludeLog = true;
      }

      if (shouldIncludeLog) {
        result.push(log);
      }
    }

    return result;
  }
}
