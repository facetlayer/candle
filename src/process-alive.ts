import { deleteProcessEntry, type ProcessEntry } from './database/processTable.ts';

/**
 * Check whether a process with the given PID is currently alive.
 * Uses signal 0 which doesn't actually send a signal — it just checks existence.
 */
export function isProcessAlive(pid: number): boolean {
  try {
    process.kill(pid, 0);
    return true;
  } catch (err: any) {
    if (err.code === 'EPERM') {
      // Process exists but belongs to another user — treat as alive.
      return true;
    }
    // ESRCH = no such process
    return false;
  }
}

/**
 * Filter out processes whose PIDs are no longer alive, removing stale
 * entries from the database. This handles the case where a reboot or
 * external kill left behind DB records with no matching OS process.
 */
export function filterAliveProcesses(entries: ProcessEntry[]): ProcessEntry[] {
  return entries.filter(entry => {
    if (entry.log_collector_pid && isProcessAlive(entry.log_collector_pid)) {
      return true;
    }
    if (isProcessAlive(entry.pid)) {
      return true;
    }
    deleteProcessEntry({
      commandName: entry.command_name,
      projectDir: entry.project_dir,
      pid: entry.pid,
    });
    return false;
  });
}
