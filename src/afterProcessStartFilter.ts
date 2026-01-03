import { ProcessLogType, type ProcessLog } from './logs/processLogs.ts';

/**
 * AfterProcessStartLogFilter
 *
 * Filters logs to only show logs after the process_start_initiated event
 * for each command. This is useful when watching multiple processes to ensure
 * we only show logs from the current run of each process.
 */
export class AfterProcessStartLogFilter {
  private startedCommands = new Set<string>();

  filter(logs: ProcessLog[]): ProcessLog[] {
    const result: ProcessLog[] = [];

    for (const log of logs) {
      const commandName = log.command_name;

      if (log.log_type === ProcessLogType.process_start_initiated) {
        this.startedCommands.add(commandName);
        result.push(log);
        continue;
      }

      if (this.startedCommands.has(commandName)) {
        result.push(log);
      }
    }

    return result;
  }
}
