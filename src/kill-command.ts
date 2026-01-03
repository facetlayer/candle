import { findProjectDir, getServiceInfoByName } from './configFile.ts';
import {
  deleteProcessEntry,
  findAllProcesses,
  findProcessesByCommandNameAndProjectDir,
  findProcessesByProjectDir,
  updateProcessKilledAt,
} from './database/processTable.ts';
import { killProcessTree } from './killProcessTree.ts';

interface KillCommandOptions {
  commandNames?: string[]; // Names of the commands to kill
  quietFailure?: boolean; // If enabled, don't return an error if no processes are found
  allGlobalServices?: boolean; // If enabled, kill all global services
  quiet?: boolean; // If enabled, don't print any output
}

export async function handleKill(options: KillCommandOptions) {
  const commandNames = options.commandNames ?? [];

  // If specific names provided, kill each one
  if (commandNames.length > 0 && !options.allGlobalServices) {
    for (const commandName of commandNames) {
      await killByName(commandName, options);
    }
    return;
  }

  // Kill all local or global services
  let projectDir: string | undefined;

  if (options.allGlobalServices) {
    // When killing all global services, we don't need projectDir
  } else {
    // When killing all local services, we only need the projectDir
    projectDir = findProjectDir();
  }

  const runningProcesses = options.allGlobalServices
    ? findAllProcesses()
    : findProcessesByProjectDir(projectDir!);

  let killedProcessCount = 0;

  for (const process of runningProcesses) {
    await killProcess(process, options);
    killedProcessCount++;
  }

  if (killedProcessCount === 0 && !options.quietFailure) {
    if (options.allGlobalServices) {
      console.log('No running processes found');
    } else {
      console.log(`No running processes found in project '${projectDir}'`);
    }
  }
}

async function killByName(commandName: string, options: KillCommandOptions) {
  // Get service info - works for both config-defined and transient processes
  let info;
  try {
    info = getServiceInfoByName(commandName);
  } catch (error) {
    // If quietFailure is enabled, silently return when service is not found
    if (options.quietFailure) {
      return;
    }
    throw error;
  }
  const projectDir = info.projectDir;
  const serviceName = info.commandName;

  const runningProcesses = findProcessesByCommandNameAndProjectDir(serviceName, projectDir);

  let killedProcessCount = 0;

  for (const process of runningProcesses) {
    if (process.command_name !== serviceName) {
      continue;
    }
    await killProcess(process, options);
    killedProcessCount++;
  }

  if (killedProcessCount === 0 && !options.quietFailure) {
    console.log(
      `No running processes found for service '${serviceName}' in project '${projectDir}'`
    );
  }
}

async function killProcess(
  process: { command_name: string; project_dir: string; pid: number | null; killed_at?: number | null },
  options: KillCommandOptions
) {
  // Kill the process and its entire process tree
  if (process.pid) {
    const result = await killProcessTree(process.pid);
    if (result === 'success') {
      if (!options.quiet) {
        console.log(`[Killed '${process.command_name}' process with PID: ${process.pid}]`);
      }

      // If the killed_at date is over 5 minutes old, delete the stale entry.
      // This can happen if the entry fails to get cleaned up.

      if (process.killed_at && process.killed_at < Date.now() - 5 * 60 * 1000) {
        if (!options.quiet) {
          console.warn(
            `[Cleaning up stale process entry for '${process.command_name}' with PID: ${process.pid}]`
          );
        }
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
      if (!options.quiet) {
        console.warn(
          `[Cleaning up stale process entry for '${process.command_name}' with PID: ${process.pid}]`
        );
      }
      deleteProcessEntry({
        commandName: process.command_name,
        projectDir: process.project_dir,
        pid: process.pid,
      });
    } else {
      if (!options.quiet) {
        console.log(`Error killing process '${process.command_name}' with PID: ${process.pid}`);
      }
    }
  }
}
