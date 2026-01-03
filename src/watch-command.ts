import { getServiceInfoByName } from './configFile.ts';
import { watchProcess } from './watchProcess.ts';

interface WatchCommandOptions {
  commandNames: string[]; // Names of the commands to watch
}

export async function handleWatch(options: WatchCommandOptions): Promise<void> {
  // If no names provided, use default
  const namesToWatch = options.commandNames.length > 0 ? options.commandNames : [null];

  const foundProcesses: { serviceName: string; pid: number; projectDir: string }[] = [];

  for (const name of namesToWatch) {
    // getServiceInfoByName handles both config-defined and transient processes
    const info = getServiceInfoByName(name);

    if (!info.runningProcess) {
      console.log(
        `No running process found for command '${info.commandName}' in project '${info.projectDir}'.`
      );
      continue;
    }

    foundProcesses.push({
      serviceName: info.commandName,
      pid: info.runningProcess.pid,
      projectDir: info.projectDir,
    });
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

  // All processes should be in the same project directory
  const projectDir = foundProcesses[0].projectDir;
  const serviceNames = foundProcesses.map(p => p.serviceName);

  // Start watching the processes
  await watchProcess({
    projectDir,
    commandNames: serviceNames,
    consoleOutputFormat: 'pretty',
  });
}
