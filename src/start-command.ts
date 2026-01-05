import { UsageError } from './errors.ts';
import { startOneService, type StartResult } from './start/startOneService.ts';

export { startOneService, type StartResult };

interface StartOptions {
  projectDir: string;
  commandNames?: string[]; // names of the services to start
  consoleOutputFormat: 'pretty' | 'json';
  shell?: string;
  root?: string;
  pty?: boolean;
  enableStdin?: boolean;
}

/*
 handleStart

 Launches one or more services.

 Exits immediately, does not watch logs or wait for processes to exit.
*/

export async function handleStartCommand(req: StartOptions) {
  if (!req.projectDir) {
    throw new Error('handleStartCommand: projectDir is required');
  }

  let commandNames = req.commandNames || [];

  // If shell is provided, we're starting a transient process
  if (req.shell) {
    if (commandNames.length !== 1 || !commandNames[0]) {
      throw new UsageError('Exactly one service name is required when using --shell');
    }

    await startOneService({
      projectDir: req.projectDir,
      commandName: commandNames[0],
      consoleOutputFormat: req.consoleOutputFormat,
      shell: req.shell,
      root: req.root,
      pty: req.pty,
      enableStdin: req.enableStdin,
    });
    return;
  }

  // If no service names provided, start the default service
  if (commandNames.length === 0) {
    commandNames = [null];
  }

  for (const commandName of commandNames) {
    await startOneService({
      projectDir: req.projectDir,
      commandName: commandName,
      consoleOutputFormat: req.consoleOutputFormat,
    });
  }
}
