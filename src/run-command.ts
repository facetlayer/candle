import { startOneService } from './start-command.ts';
import { watchProcess } from './watchProcess.ts';

interface RunOptions {
  projectDir: string;
  commandNames: string[];
  shell?: string;
  root?: string;
  pty?: boolean;
}

export async function handleRunCommand(req: RunOptions): Promise<void> {
  if (!req.projectDir) {
    throw new Error('handleStartCommand: projectDir is required');
  }

  const { projectDir, commandNames, shell, root, pty } = req;

  let namesToRun = commandNames.length > 0 ? commandNames : [null]; // null means default service

  // If shell is provided, only one service name is allowed
  if (shell && namesToRun.length > 1) {
    console.error('Error: --shell can only be used with a single service name');
    process.exit(1);
  }

  const startedServices: { projectDir: string; serviceName: string }[] = [];
  for (const name of namesToRun) {
    const result = await startOneService({
      projectDir,
      commandName: name,
      consoleOutputFormat: 'pretty',
      shell,
      root,
      pty,
    });
    startedServices.push(result);
  }

  console.log('[Now watching logs - Press Ctrl+C to exit.]');

  await watchProcess({ projectDir, commandNames, consoleOutputFormat: 'pretty' });
  process.exit(0);
}
