import { getServiceConfigByName } from './configFile.ts';
import { deleteProcessEntry, findAllProcesses, findProcessesByCommandNameAndProjectDir, findProcessesByProjectDir, updateProcessKilledAt } from './database/processTable.ts';
import { killProcessTree } from './killProcessTree.ts';

interface KillCommandOptions {
    commandName?: string; // Name of the command to kill
    quietFailure?: boolean; // If enabled, don't return an error if no processes are found
    allGlobalServices?: boolean; // If enabled, kill all global services
}   

export async function handleKill(options: KillCommandOptions) {
    const { projectDir, serviceConfig } = getServiceConfigByName(options.commandName);
    const allLocalServices = !options.commandName;

    // Find running processes using projectDir and commandName
    
    const runningProcesses = 
        options.allGlobalServices ? 
            findAllProcesses() :
        (allLocalServices ? 
            findProcessesByProjectDir(projectDir) :
            findProcessesByCommandNameAndProjectDir(serviceConfig.name, projectDir));

    let killedProcessCount = 0;
    
    for (const process of runningProcesses) {
        if (!allLocalServices && process.command_name !== serviceConfig.name) {
            continue;
        }

        // Kill the process and its entire process tree
        if (process.pid) {
            const result = await killProcessTree(process.pid);
            if (result === 'success') {
                console.log(`Killed '${process.command_name}' process with PID: ${process.pid}`);

                // If the killed_at date is over 5 minutes old, delete the stale entry.
                // This can happen if the entry fails to get cleaned up.

                if (process.killed_at && process.killed_at < Date.now() - 5 * 60 * 1000) {
                    console.warn(`Cleaning up stale process entry for '${process.command_name}' with PID: ${process.pid}`);
                    deleteProcessEntry({
                        commandName: process.command_name,
                        projectDir: process.project_dir,
                        pid: process.pid,
                    });
                } else {
                updateProcessKilledAt({
                    commandName: process.command_name,
                    projectDir: process.project_dir,
                    pid: process.pid,
                        killedAt: Math.floor(Date.now() / 1000),
                    });
                }
            } else if (result === 'process_not_found') {
                console.warn(`Cleaning up stale process entry for '${process.command_name}' with PID: ${process.pid}`);
                deleteProcessEntry({
                    commandName: process.command_name,
                    projectDir: process.project_dir,
                    pid: process.pid,
                });
            } else {
                console.log(`Error killing process '${process.command_name}' with PID: ${process.pid}`);
            }
        }

        killedProcessCount++;
    }

    if (killedProcessCount === 0 && !options.quietFailure) {
        if (allLocalServices) {
            console.log(`No running processes found in project '${projectDir}'`);
        } else {
            console.log(`No running processes found for service '${serviceConfig.name}' in project '${projectDir}'`);
        }
    }
}

