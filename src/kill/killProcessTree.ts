import treeKill from 'tree-kill';

type KillProcessTreeResult = 'success' | 'process_not_found' | 'error';

export async function killProcessTree(pid: number): Promise<KillProcessTreeResult> {
  if (pid == null || pid == 0) {
    throw new Error(`internal error: tryKillProcessTree called with invalid PID: ${pid}`);
  }

  return new Promise(resolve => {
    treeKill(pid, 'SIGTERM', (error: any) => {
      if (error && error.code === 'ESRCH') {
        // Process does not exist
        resolve('process_not_found');
      } else if (error) {
        console.warn(`Warning: Could not kill process tree for PID ${pid}:`, error.message);
        resolve('error');
      } else {
        resolve('success');
      }
    });
  });
}
