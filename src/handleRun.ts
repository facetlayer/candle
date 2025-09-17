import * as Db from './database/database.ts';
import { watchProcess } from './watchProcess.ts';
import { handleKill } from './handleKill.ts';
import { getServiceConfigByName } from './configFile.ts';
import { ProcessLogType, saveProcessLog } from './logs/processLogs.ts';
import { LogIterator } from './logs/LogIterator.ts';
import { launchWithLogCollector } from './log-collector/launchWithLogCollector.ts';
import * as Path from "node:path";

interface RunOptions {
    commandName: string 
    consoleOutputFormat: 'pretty' | 'json'
    watchLogs?: boolean
}

interface StartOptions {
    commandNames?: string[] // names of the services to start
    consoleOutputFormat: 'pretty' | 'json'
}

async function killExistingProcess(projectDir: string, commandName: string) {
    const db = Db.getDatabase();
    const foundProcesses = db.list(
        'select * from processes where project_dir = ? and command_name = ?',
        [projectDir, commandName]
    );

    for (const process of foundProcesses) {
        await handleKill({ commandName: process.command_name });
    }
}

async function waitForProcessToStart(commandName: string, projectDir: string){
    const logIterator = new LogIterator({
        commandName: commandName,
        projectDir: projectDir,
    });

    for (;;) {
        for (const log of logIterator.getNextLogs()) {
            if (log.log_type === ProcessLogType.process_started) {
                return;
            }

            if (log.log_type === ProcessLogType.process_start_failed) {
                throw new Error(`Process ${commandName} failed to start`);
            }
        }
        await new Promise(resolve => setTimeout(resolve, 100));
    }
}

/*
 handleStart

 Launches a single service as a subprocess.

 Has an option to keep running to watch logs.
*/
export async function handleRun(req: RunOptions) {
    const { projectDir, serviceConfig } = getServiceConfigByName(req.commandName);

    await killExistingProcess(projectDir, serviceConfig.name);

    // Save the 'process has started' log.
    saveProcessLog({
        command_name: serviceConfig.name,
        project_dir: projectDir,
        log_type: ProcessLogType.process_start_initiated
    });

    launchWithLogCollector(serviceConfig.name, projectDir);

    await waitForProcessToStart(serviceConfig.name, projectDir);

    let launchDir = projectDir;
    if (serviceConfig.root) {
        launchDir = Path.join(projectDir, serviceConfig.root);
    }
    if (req.watchLogs) {
        console.log(`Started '${serviceConfig.name}' (\`${serviceConfig.shell}\`) in directory: '${launchDir}'. Press Ctrl+C to exit.`);
    } else {
        console.log(`Started '${serviceConfig.name}' (\`${serviceConfig.shell}\`) in directory: '${launchDir}'.`);
    }

    if (req.watchLogs) {
        await watchProcess({
            projectDir,
            commandName: serviceConfig.name,
            consoleOutputFormat: req.consoleOutputFormat
        });
    }
}

/*
 handleStart

 Launches one or more services.

 Exits immediately, does not watch logs or wait for processes to exit.
*/

export async function handleStart(req: StartOptions) {
    let commandNames = req.commandNames || [];
    
    // If no service names provided, start the default service
    if (commandNames.length === 0) {
        commandNames = [null];
    }

    for (const commandName of commandNames) {
        await handleRun({
            commandName: commandName,
            consoleOutputFormat:
            req.consoleOutputFormat
        });
    }
    
}

