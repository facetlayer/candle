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

  /**
   * If set, logs are additionally pruned so that the oldest log shown has a timestamp no
   * older than (now - recentWindowMs) at the time checkLatestLaunchStatus() was called.
   *
   * This trims noisy history when watching a long-running service: if the most recent launch
   * happened before the window, only logs within the window are shown; if the launch happened
   * within the window, logs are shown starting from the launch.
   */
  recentWindowMs?: number;
}

/**
 * LatestExecutionLogFilter
 *
 * Filters logs to only show logs from the most recent process launch for each command.
 * Optionally also applies a recency window (see recentWindowMs).
 *
 * Usage:
 * 1. First call checkLatestLaunchStatus(logs) with the existing recent logs.
 * 2. Call filter(logs) to get only logs from the most recent launch
 */
export class LatestExecutionLogFilter {
  recentCommandLaunch = new Map<string, LaunchStatus>();
  private showPastLogsBehavior: ShowPastLogsBehavior;
  private recentWindowMs?: number;
  private minTimestamp?: number;

  constructor(options: LatestExecutionLogFilterOptions) {
    this.showPastLogsBehavior = options.showPastLogsBehavior;
    this.recentWindowMs = options.recentWindowMs;
  }

  /**
   * Analyze logs to determine the latest launch status for each command.
   * Logs should be in chronological order (oldest first).
   *
   * This method scans logs to find the most recent launch status for each command,
   * and (if recentWindowMs was set) snapshots the current time to compute the
   * minimum timestamp that logs must exceed in order to be shown.
   */
  checkLatestLaunchStatus(logs: ProcessLog[]): void {
    this.recentCommandLaunch.clear();

    if (this.recentWindowMs !== undefined) {
      this.minTimestamp = Date.now() - this.recentWindowMs;
    }

    for (const log of logs) {
      const commandName = log.command_name;

      if (log.log_type === ProcessLogType.process_start_initiated) {
        this.recentCommandLaunch.set(commandName, {
          startLogId: log.id,
        });
      }
    }
  }

  private passesTimestampWindow(log: ProcessLog): boolean {
    if (this.minTimestamp === undefined) {
      return true;
    }
    return log.timestamp >= this.minTimestamp;
  }

  /**
   * Filter logs to only include logs from the most recent launch for each command.
   * Only includes logs with id >= the startLogId determined by checkLatestLaunchStatus.
   *
   * Additionally, if recentWindowMs was set, logs older than (now - recentWindowMs) at
   * the time of checkLatestLaunchStatus() are excluded.
   */
  filter(logs: ProcessLog[]): ProcessLog[] {
    const result: ProcessLog[] = [];

    for (const log of logs) {
      const commandName = log.command_name;
      const status = this.recentCommandLaunch.get(commandName);

      let shouldIncludeLog = false;

      if (status) {
        // We found a start event - only include logs from that point forward
        if (log.id >= status.startLogId && this.passesTimestampWindow(log)) {
          shouldIncludeLog = true;
        }
      } else {
        // No start event found yet for this command
        if (log.log_type === ProcessLogType.process_start_initiated) {
          // Found a start event - mark it and include this log (subject to time window)
          this.recentCommandLaunch.set(commandName, {
            startLogId: log.id,
          });
          shouldIncludeLog = this.passesTimestampWindow(log);
        } else if (this.showPastLogsBehavior === 'show_logs_from_previous_launch') {
          // No start event, but configured to show existing logs anyway
          shouldIncludeLog = this.passesTimestampWindow(log);
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
