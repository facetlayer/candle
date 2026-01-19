import { getProcessTree } from '../process-tree.ts';

type KillProcessTreeResult = 'success' | 'process_not_found' | 'error';

export async function killProcessTree(pid: number): Promise<KillProcessTreeResult> {
  if (pid == null || pid == 0) {
    throw new Error(`internal error: tryKillProcessTree called with invalid PID: ${pid}`);
  }

  const pids = await getProcessTree(pid);

  if (pids.length === 0) {
    return 'process_not_found';
  }

  let hasError = false;
  let allNotFound = true;

  // Kill children first (reverse order), then the root
  for (const childPid of pids.reverse()) {
    try {
      process.kill(childPid, 'SIGTERM');
      allNotFound = false;
    } catch (err: any) {
      if (err.code === 'ESRCH') {
        // Process doesn't exist, continue
      } else {
        console.warn(`Warning: Could not kill process ${childPid}:`, err.message);
        hasError = true;
      }
    }
  }

  if (allNotFound) {
    return 'process_not_found';
  }

  return hasError ? 'error' : 'success';
}
