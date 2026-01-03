import * as Path from 'node:path';
import { findProjectDir, getServiceConfigByName, type ServiceConfig } from '../configFile.ts';
import { ProcessStartFailedError, UsageError } from '../errors.ts';
import { handleKill } from '../kill-command.ts';
import { launchWithLogCollector } from '../log-collector/launchWithLogCollector.ts';
import { LogIterator } from '../logs/LogIterator.ts';
import { saveProcessLog } from '../logs/processLogs.ts';
import { ProcessLogType } from '../logs/ProcessLogType.ts';
import { debugLog } from '../debug.ts';

export interface RunOptions {
  commandName: string;
  consoleOutputFormat: 'pretty' | 'json';
  shell?: string;
  root?: string;
}

export interface StartResult {
  projectDir: string;
  serviceName: string;
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

/*
 startOneService

 Launches a single service as a subprocess.
*/
export async function startOneService(req: RunOptions): Promise<StartResult> {
  let projectDir: string;
  let serviceConfig: ServiceConfig;
  const startTime = Date.now();

  function getTimeSinceStart() {
    return `${Date.now() - startTime}ms`;
  }

  debugLog('[startOneService] starting: ' + JSON.stringify(req));

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

  debugLog('[startOneService] killed existing processes, timeSinceStart=' + getTimeSinceStart());

  const logIterator = new LogIterator({
    commandNames: [serviceConfig.name],
    projectDir: projectDir,
  });
  logIterator.resetToLatestLogMessage();

  const initialLogPosition = logIterator.copy();

  // Save the 'process has started' log.
  saveProcessLog({
    command_name: serviceConfig.name,
    project_dir: projectDir,
    log_type: ProcessLogType.process_start_initiated,
  });

  debugLog('[startOneService] launching with log collector for name=' + serviceConfig.name + ', projectDir=' + projectDir
    + ', timeSinceStart=' + getTimeSinceStart());

  await launchWithLogCollector({
    commandName: serviceConfig.name,
    projectDir,
    shell: serviceConfig.shell,
    root: serviceConfig.root,
  });

  debugLog('[startOneService] waiting for process start logs, timeSinceStart=' + getTimeSinceStart());

  // Watch the logs for either 'process_started' or 'process_start_failed'
  const waitForSuccess = (async () => {
    for await (const log of logIterator.it()) {
      debugLog('[startOneService] saw log while waiting: ' + JSON.stringify(log));

      if (log.log_type === ProcessLogType.process_started) {
        // success
        break;
      }

      if (log.log_type === ProcessLogType.process_start_failed) {
        const recentLogs = initialLogPosition.getNextLogs();
        throw new ProcessStartFailedError({ commandName: serviceConfig.name, recentLogs });
      }
    }
  })();

  // Wait on the logs with a timeout
  await Promise.race([
    waitForSuccess,
    new Promise((resolve, reject) => {
      setTimeout(() => {
        reject(new Error('Process failed to start (timed out while waiting)'));
      }, 10000);
    }),
  ]);

  debugLog('[startOneService] finished startup, timeSinceStart=' + getTimeSinceStart());

  let launchDir = projectDir;
  if (serviceConfig.root) {
    launchDir = Path.join(projectDir, serviceConfig.root);
  }
  console.log(
    `[Started process '${serviceConfig.name}' (\`${serviceConfig.shell}\`) in directory: '${launchDir}']`
  );

  return {
    projectDir,
    serviceName: serviceConfig.name,
  };
}
