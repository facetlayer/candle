import { isProcessAlive } from '../process-alive.ts';
import { deleteProcessEntry, findAllRunningProcesses, findAllKilledProcesses } from './processTable.ts';
import { saveProcessLog } from '../logs/processLogs.ts';
import { ProcessLogType } from '../logs/ProcessLogType.ts';

/**
 * Find process entries in the database that claim to be running but whose PIDs
 * are actually dead. This handles the case where the system was rebooted (or
 * processes were killed externally) and the log collector never got a chance to
 * clean up.
 *
 * Also cleans up entries that were marked as killed (killed_at set) but not yet
 * deleted — this happens when the log collector dies before it can clean up.
 */
export function cleanupStaleProcesses(): void {
  // Clean up entries with no killed_at where both PIDs are dead
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

  // Clean up entries that were marked as killed but the log collector never deleted.
  // The log collector deletes entries on process exit, but if it died first, these linger.
  const killedProcesses = findAllKilledProcesses();
  for (const proc of killedProcesses) {
    deleteProcessEntry({
      commandName: proc.command_name,
      projectDir: proc.project_dir,
      pid: proc.pid,
    });
  }
}
