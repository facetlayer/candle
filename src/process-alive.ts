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
