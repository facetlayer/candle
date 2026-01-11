import { UsageError } from './errors.ts';
import { startOneService } from './start-command.ts';
import { watchProcess } from './watchProcess.ts';

interface RunOptions {
  projectDir: string;
  commandNames: string[];
  shell?: string;
  root?: string;
  enableStdin?: boolean;
}

export async function handleRunCommand(req: RunOptions): Promise<void> {
  if (!req.projectDir) {
    throw new Error('handleStartCommand: projectDir is required');
  }

  const { projectDir, commandNames, shell, root, enableStdin } = req;

  if (commandNames.length === 0) {
    throw new UsageError(`At least one service name is required for 'run' command`);
  }

  // If shell is provided, only one service name is allowed
  if (shell && commandNames.length > 1) {
    console.error('Error: --shell can only be used with a single service name');
    process.exit(1);
  }

  const startedServices: { projectDir: string; serviceName: string }[] = [];
  for (const name of commandNames) {
    const result = await startOneService({
      projectDir,
      commandName: name,
      consoleOutputFormat: 'pretty',
      shell,
      root,
      enableStdin,
    });
    startedServices.push(result);
  }

  console.log('[Now watching logs - Press Ctrl+C to exit.]');

  const serviceNamesToWatch = startedServices.map(s => s.serviceName);
  await watchProcess({
    projectDir,
    commandNames: serviceNamesToWatch,
    consoleOutputFormat: 'pretty',
    showPastLogsBehavior: 'only_show_after_recent_launch',
  });
  process.exit(0);
}
