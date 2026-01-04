import { findAllProcesses } from './database/processTable.ts';
import { killOneRunningProcess, type KillProcessOptions } from './kill/killOneRunningProcess.ts';

export async function handleKillAll(options: KillProcessOptions = {}) {
  const runningProcesses = findAllProcesses();

  let killedProcessCount = 0;

  for (const process of runningProcesses) {
    await killOneRunningProcess(process, options);
    killedProcessCount++;
  }

  if (killedProcessCount === 0) {
    console.log('No running processes found');
  }
}
