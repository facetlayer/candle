import { findConfigFile, findProjectDir, getAllServiceNames } from './configFile.ts';
import { findProcessesByCommandNameAndProjectDir } from './database/processTable.ts';
import { startOneService } from './start/startOneService.ts';
import { watchProcess } from './watchProcess.ts';

interface WatchCommandOptions {
  commandNames: string[]; // Names of the commands to watch
}

export async function handleWatch(options: WatchCommandOptions): Promise<void> {
  const projectDir = findProjectDir();
  let commandNames = options.commandNames;

  if (commandNames.length === 0) {
    // No names provided - treat like `start`: launch all configured services.
    const { config } = findConfigFile(projectDir);
    commandNames = getAllServiceNames(config);
    if (commandNames.length === 0) {
      console.log('No services configured in .candle.json');
      return;
    }
  }

  // Ensure each service is running. Skip those already running; start the rest.
  for (const name of commandNames) {
    const existing = findProcessesByCommandNameAndProjectDir(name, projectDir)
      .filter(p => p.killed_at === null);
    if (existing.length > 0) {
      console.log(`[Service '${name}' is already running]`);
      continue;
    }
    await startOneService({
      projectDir,
      commandName: name,
      consoleOutputFormat: 'pretty',
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
  });
}
