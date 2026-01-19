import { spawn } from 'child_process';

/**
 * Get all PIDs in a process tree, including the root PID and all descendants.
 * Uses platform-specific commands:
 * - macOS: pgrep -P <PID>
 * - Linux: ps -o pid --no-headers --ppid <PID>
 */
export async function getProcessTree(rootPid: number): Promise<number[]> {
  const allPids: number[] = [rootPid];
  const toVisit: number[] = [rootPid];

  while (toVisit.length > 0) {
    const pid = toVisit.pop()!;
    const children = await getChildPids(pid);
    for (const child of children) {
      allPids.push(child);
      toVisit.push(child);
    }
  }

  return allPids;
}

async function getChildPids(parentPid: number): Promise<number[]> {
  const platform = process.platform;

  if (platform === 'darwin') {
    return runCommandForPids('pgrep', ['-P', parentPid.toString()]);
  } else if (platform === 'linux') {
    return runCommandForPids('ps', ['-o', 'pid', '--no-headers', '--ppid', parentPid.toString()]);
  } else {
    return [];
  }
}

async function runCommandForPids(command: string, args: string[]): Promise<number[]> {
  return new Promise((resolve) => {
    const child = spawn(command, args, {
      stdio: ['ignore', 'pipe', 'ignore'],
    });

    let stdout = '';
    child.stdout.on('data', (data) => {
      stdout += data.toString();
    });

    child.on('close', () => {
      const pids = stdout
        .trim()
        .split('\n')
        .filter((line) => line.length > 0)
        .map((line) => parseInt(line.trim(), 10))
        .filter((pid) => !isNaN(pid));
      resolve(pids);
    });

    child.on('error', () => {
      resolve([]);
    });
  });
}
