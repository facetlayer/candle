import { type ProcessLog } from '../logs/processLogs.ts';
import { ProcessLogType } from '../logs/ProcessLogType.ts';

interface LaunchStatus {
  startLogId: number;
}

type ShowPastLogsBehavior = 'show_logs_from_previous_launch' | 'only_show_after_recent_launch';

interface LatestExecutionLogFilterOptions {
  /**
   * What to do if no recent launch event is found in the logs:
   * - 'show_logs_from_previous_launch': Show all logs anyway (useful for `logs` and `watch` commands)
   * - 'only_show_after_recent_launch': Only show logs after finding a start event (useful for `run`)
   */
  showPastLogsBehavior: ShowPastLogsBehavior;
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
  private showPastLogsBehavior: ShowPastLogsBehavior;

  constructor(options: LatestExecutionLogFilterOptions) {
    this.showPastLogsBehavior = options.showPastLogsBehavior;
  }

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
   *
   * Behavior when no start event is found depends on ifRecentLaunchNotFound option:
   * - 'show_existing_logs': Include all logs (for viewing historical output)
   * - 'only_show_after_recent_launch': Exclude logs until a start event is found
   */
  filter(logs: ProcessLog[]): ProcessLog[] {
    const result: ProcessLog[] = [];

    for (const log of logs) {
      const commandName = log.command_name;
      const status = this.recentCommandLaunch.get(commandName);

      let shouldIncludeLog = false;

      if (status) {
        // We found a start event - only include logs from that point forward
        if (log.id >= status.startLogId) {
          shouldIncludeLog = true;
        }
      } else {
        // No start event found yet for this command
        if (log.log_type === ProcessLogType.process_start_initiated) {
          // Found a start event - mark it and include this log
          this.recentCommandLaunch.set(commandName, {
            startLogId: log.id,
          });
          shouldIncludeLog = true;
        } else if (this.showPastLogsBehavior === 'show_logs_from_previous_launch') {
          // No start event, but configured to show existing logs anyway
          shouldIncludeLog = true;
        }
        // If 'only_show_after_recent_launch', shouldIncludeLog stays false
      }

      if (shouldIncludeLog) {
        result.push(log);
      }
    }

    return result;
  }
}
