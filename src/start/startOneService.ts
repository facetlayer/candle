import * as Path from 'node:path';
import { findProjectDir, getServiceConfigByName, type ServiceConfig } from '../configFile.ts';
import { UsageError } from '../errors.ts';
import { handleKill } from '../kill-command.ts';
import { launchWithLogCollector } from '../log-collector/launchWithLogCollector.ts';
import { LogIterator } from '../logs/LogIterator.ts';
import { ProcessLogType, saveProcessLog } from '../logs/processLogs.ts';
import { watchProcess } from '../watchProcess.ts';
import { getDatabase } from '../database/database.ts';

export interface RunOptions {
  commandName: string;
  consoleOutputFormat: 'pretty' | 'json';
  watchLogs?: boolean;
  shell?: string;
  root?: string;
}

function isValidRelativePath(p: string): boolean {
  if (Path.isAbsolute(p)) {
    return false;
  }
  const normalized = Path.normalize(p);
  if (normalized.startsWith('..')) {
    return false;
  }
  return true;
}

async function waitForProcessToStart(commandName: string, projectDir: string) {
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
 startOneService

 Launches a single service as a subprocess.

 Has an option to keep running to watch logs.
*/
export async function startOneService(req: RunOptions) {
  let projectDir: string;
  let serviceConfig: ServiceConfig;

  if (req.shell) {
    // Transient process - use provided shell/root
    if (!req.commandName) {
      throw new UsageError('Commad name is required');
    }

    // Validate root if provided
    if (req.root && !isValidRelativePath(req.root)) {
      throw new UsageError(
        `Invalid root path: "${req.root}". Root must be a relative path within the project.`
      );
    }

    // Still need a config file to define the project boundary
    projectDir = findProjectDir();

    serviceConfig = {
      name: req.commandName,
      shell: req.shell,
      root: req.root,
    };
  } else {
    // Config-based process
    const found = getServiceConfigByName(req.commandName);
    projectDir = found.projectDir;
    serviceConfig = found.serviceConfig;
  }

  // Kill any existing processes for this service
  await handleKill({ commandNames: [serviceConfig.name], quietFailure: true });

  // Save the 'process has started' log.
  saveProcessLog({
    command_name: serviceConfig.name,
    project_dir: projectDir,
    log_type: ProcessLogType.process_start_initiated,
  });

  await launchWithLogCollector({
    commandName: serviceConfig.name,
    projectDir,
    shell: serviceConfig.shell,
    root: serviceConfig.root,
  });

  await waitForProcessToStart(serviceConfig.name, projectDir);

  let launchDir = projectDir;
  if (serviceConfig.root) {
    launchDir = Path.join(projectDir, serviceConfig.root);
  }
  if (req.watchLogs) {
    console.log(
      `Started '${serviceConfig.name}' (\`${serviceConfig.shell}\`) in directory: '${launchDir}'. Press Ctrl+C to exit.`
    );
  } else {
    console.log(
      `Started '${serviceConfig.name}' (\`${serviceConfig.shell}\`) in directory: '${launchDir}'.`
    );
  }

  if (req.watchLogs) {
    await watchProcess({
      projectDir,
      commandName: serviceConfig.name,
      consoleOutputFormat: req.consoleOutputFormat,
    });
  }
}
