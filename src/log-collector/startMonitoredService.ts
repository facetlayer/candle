import { startShellCommand, Subprocess } from '@facetlayer/subprocess-wrapper';
import * as Path from 'node:path';
import type { LogCollectorLaunchInfo } from './LogCollectorLaunchInfo.ts';
import { saveProcessLog } from '../logs/processLogs.ts';
import { ProcessLogType } from '../logs/ProcessLogType.ts';
import { debugLog } from '../debug.ts';

const VERY_VERBOSE_LOGS = true;

export function startMonitoredService(message: LogCollectorLaunchInfo): Subprocess {
  const { commandName, projectDir, shell, root } = message;


  let launchDir = projectDir;
  if (root) {
    launchDir = Path.join(projectDir, root);
  }

  if (VERY_VERBOSE_LOGS) {
    debugLog('[startService] starting shell command ' + JSON.stringify(shell, null, 2));
  }

  const subprocess = startShellCommand(shell, [], {
    shell: true,
    cwd: launchDir,
    onStdout: line => {
      saveProcessLog({
        command_name: commandName,
        project_dir: projectDir,
        content: line,
        log_type: ProcessLogType.stdout,
      });
      if (VERY_VERBOSE_LOGS) {
        debugLog('[startService] stdout: ' + line);
      }
    },
    onStderr: line => {
      saveProcessLog({
        command_name: commandName,
        project_dir: projectDir,
        content: line,
        log_type: ProcessLogType.stderr,
      });
      if (VERY_VERBOSE_LOGS) {
        debugLog('[startService] stderr: ' + line);
      }
    },
  });

  if (VERY_VERBOSE_LOGS) {
    subprocess.proc.stdout.on('data', line => {
      debugLog('[startService] subprocess stdout data: ' + line);
    });
  }

  return subprocess;
}
