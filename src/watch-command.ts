import { findProjectDir } from './configFile.ts';
import {
  findProcessesByCommandNameAndProjectDir,
  findProcessesByProjectDir,
} from './database/processTable.ts';
import { watchProcess } from './watchProcess.ts';

interface WatchCommandOptions {
  commandNames: string[]; // Names of the commands to watch
}

export async function handleWatch(options: WatchCommandOptions): Promise<void> {
  const projectDir = findProjectDir();
  const commandNames = options.commandNames;

  const foundProcesses: { serviceName: string; pid: number; projectDir: string }[] = [];

  if (commandNames.length > 0) {
    // Watch specific command names
    for (const name of commandNames) {
      const runningProcesses = findProcessesByCommandNameAndProjectDir(name, projectDir);

      if (runningProcesses.length === 0) {
        console.log(`No running process found for command '${name}' in project '${projectDir}'.`);
        continue;
      }

      foundProcesses.push({
        serviceName: name,
        pid: runningProcesses[0].pid,
        projectDir,
      });
    }
  } else {
    // No names provided - watch all running processes in the project
    const runningProcesses = findProcessesByProjectDir(projectDir);

    for (const process of runningProcesses) {
      foundProcesses.push({
        serviceName: process.command_name,
        pid: process.pid,
        projectDir,
      });
    }
  }

  if (foundProcesses.length === 0) {
    console.log('No running processes found to watch.');
    console.log('');
    return;
  }

  // Print what we're watching
  if (foundProcesses.length === 1) {
    console.log(`Watching process '${foundProcesses[0].serviceName}' (PID: ${foundProcesses[0].pid})`);
  } else {
    console.log(`Watching ${foundProcesses.length} processes:`);
    for (const proc of foundProcesses) {
      console.log(`  - '${proc.serviceName}' (PID: ${proc.pid})`);
    }
  }
  console.log('Press Ctrl+C to stop watching.');
  console.log('');

  const serviceNames = foundProcesses.map(p => p.serviceName);

  // Start watching the processes
  await watchProcess({
    projectDir,
    commandNames: serviceNames,
    consoleOutputFormat: 'pretty',
  });
}
