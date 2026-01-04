import { findProcessesByCommandNameAndProjectDir, findProcessesByProjectDir } from './database/processTable.ts';
import { killOneRunningProcess, type KillProcessOptions } from './kill/killOneRunningProcess.ts';

interface KillCommandOptions extends KillProcessOptions {
  projectDir: string;
  commandNames?: string[]; // Names of the commands to kill
  quietFailure?: boolean; // If enabled, don't return an error if no processes are found
}

export async function handleKillCommand(req: KillCommandOptions) {
  const commandNames = req.commandNames ?? [];

  // If specific names provided, kill each one
  if (commandNames.length > 0) {
    for (const commandName of commandNames) {
      await killByCommandName(req.projectDir, commandName, req);
    }
    return;
  }

  // No names provided - kill all running processes in the project
  const runningProcesses = findProcessesByProjectDir(req.projectDir);

  let killedProcessCount = 0;

  for (const process of runningProcesses) {
    await killOneRunningProcess(process, req);
    killedProcessCount++;
  }

  if (killedProcessCount === 0 && !req.quietFailure) {
    console.log(`No running processes found in project '${req.projectDir}'`);
  }
}

async function killByCommandName(projectDir: string, commandName: string, options: KillCommandOptions) {
  const runningProcesses = findProcessesByCommandNameAndProjectDir(commandName, projectDir);

  let killedProcessCount = 0;

  for (const process of runningProcesses) {
    await killOneRunningProcess(process, options);
    killedProcessCount++;
  }

  if (killedProcessCount === 0 && !options.quietFailure) {
    console.log(
      `No running processes found for service '${commandName}' in project '${projectDir}'`
    );
  }
}
