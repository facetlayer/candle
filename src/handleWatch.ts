import * as Db from './database/database.ts';
import { watchProcess } from './watchProcess.ts';
import { getServiceConfigByName } from './configFile.ts';
import { findProcessesByCommandNameAndProjectDir } from './database/processTable.ts';

interface WatchCommandOptions {
    commandName: string; // Name of the command to watch
}

export async function handleWatch(options: WatchCommandOptions): Promise<void> {
    const { serviceConfig, projectDir } = getServiceConfigByName(options.commandName);

    // Find the running process to watch using projectDir and commandName
    const found = findProcessesByCommandNameAndProjectDir(serviceConfig.name, projectDir)[0];
    
    if (!found) {
        console.log(`No running process found for command '${serviceConfig.name}' in project '${projectDir}'.`);
        console.log('');
        return;
    }
    
    console.log(`Watching process '${serviceConfig.name}' (PID: ${found.pid})`);
    console.log('Press Ctrl+C to stop watching.');
    console.log('');
    
    // Start watching the process
    await watchProcess({
        projectDir,
        commandName: serviceConfig.name,
        consoleOutputFormat: 'pretty'
    });
}