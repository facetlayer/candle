import { UsageError } from './errors.ts';
import { startOneService } from './start/startOneService.ts';

export { startOneService };

interface StartOptions {
  commandNames?: string[]; // names of the services to start
  consoleOutputFormat: 'pretty' | 'json';
  shell?: string;
  root?: string;
}

/*
 handleStart

 Launches one or more services.

 Exits immediately, does not watch logs or wait for processes to exit.
*/

export async function handleStartCommand(req: StartOptions) {
  let commandNames = req.commandNames || [];

  // If shell is provided, we're starting a transient process
  if (req.shell) {
    if (commandNames.length !== 1 || !commandNames[0]) {
      throw new UsageError('Exactly one service name is required when using --shell');
    }

    await startOneService({
      commandName: commandNames[0],
      consoleOutputFormat: req.consoleOutputFormat,
      shell: req.shell,
      root: req.root,
    });
    return;
  }

  // If no service names provided, start the default service
  if (commandNames.length === 0) {
    commandNames = [null];
  }

  for (const commandName of commandNames) {
    await startOneService({
      commandName: commandName,
      consoleOutputFormat: req.consoleOutputFormat,
    });
  }
}
