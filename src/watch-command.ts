import { findProjectDir, resolveCommandNamesOrAll } from './configFile.ts';
import { startOneService } from './start/startOneService.ts';
import { watchProcess } from './watchProcess.ts';

interface WatchCommandOptions {
  commandNames: string[]; // Names of the commands to watch
  exitAfterMs?: number; // Optional: stop watching automatically after this many ms (used for testing)
}

export async function handleWatch(options: WatchCommandOptions): Promise<void> {
  const projectDir = findProjectDir();
  const { exitAfterMs } = options;
  const commandNames = resolveCommandNamesOrAll(projectDir, options.commandNames);

  // Ensure each service is running. startOneService with checkStart:true is a no-op
  // for services that are already running (including transient processes not in config).
  for (const name of commandNames) {
    await startOneService({
      projectDir,
      commandName: name,
      consoleOutputFormat: 'pretty',
      checkStart: true,
    });
  }

  // Print what we're watching
  if (commandNames.length === 1) {
    console.log(`Watching process '${commandNames[0]}'`);
  } else {
    console.log(`Watching ${commandNames.length} processes:`);
    for (const name of commandNames) {
      console.log(`  - '${name}'`);
    }
  }
  console.log('Press Ctrl+C to stop watching.');
  console.log('');

  await watchProcess({
    projectDir,
    commandNames,
    consoleOutputFormat: 'pretty',
    exitAfterMs,
  });
}
