import { isProcessAlive } from '../process-alive.ts';
import { deleteProcessEntry, findAllRunningProcesses } from './processTable.ts';
import { saveProcessLog } from '../logs/processLogs.ts';
import { ProcessLogType } from '../logs/ProcessLogType.ts';

/**
 * Find process entries in the database that claim to be running but whose PIDs
 * are actually dead. This handles the case where the system was rebooted (or
 * processes were killed externally) and the log collector never got a chance to
 * clean up.
 */
export function cleanupStaleProcesses(): void {
  const runningProcesses = findAllRunningProcesses();

  for (const proc of runningProcesses) {
    // If the log collector is still alive, it's managing this process — skip it.
    if (proc.log_collector_pid && isProcessAlive(proc.log_collector_pid)) {
      continue;
    }

    // If the service process itself is still alive, skip it.
    if (isProcessAlive(proc.pid)) {
      continue;
    }

    // Both the log collector and the service process are dead — this is stale.
    saveProcessLog({
      command_name: proc.command_name,
      project_dir: proc.project_dir,
      log_type: ProcessLogType.process_exited,
      content: 'Process cleaned up (stale entry after restart or crash)',
    });

    deleteProcessEntry({
      commandName: proc.command_name,
      projectDir: proc.project_dir,
      pid: proc.pid,
    });
  }
}
