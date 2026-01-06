import { startShellCommand, Subprocess } from '@facetlayer/subprocess-wrapper';
import * as Path from 'node:path';
import type { LogCollectorLaunchInfo } from './LogCollectorLaunchInfo.ts';
import { saveProcessLog } from '../logs/processLogs.ts';
import { ProcessLogType } from '../logs/ProcessLogType.ts';
import { debugLog } from '../debug.ts';
import { popStdinMessage, clearStdinMessages } from '../database/stdinMessagesTable.ts';

const VERY_VERBOSE_LOGS = true;

const STDIN_POLL_INTERVAL_MS = 500;

export function startMonitoredService(message: LogCollectorLaunchInfo): Subprocess {
  const { commandName, projectDir, shell, root, enableStdin } = message;

  let launchDir = projectDir;
  if (root) {
    launchDir = Path.join(projectDir, root);
  }

  if (VERY_VERBOSE_LOGS) {
    debugLog('[startService] starting shell command ' + JSON.stringify(shell, null, 2) + ' enableStdin=' + enableStdin);
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

  // Set up stdin polling if enabled
  if (enableStdin) {
    // Clear any stale stdin messages from previous runs
    clearStdinMessages(commandName, projectDir);

    const stdinPollInterval = setInterval(() => {
      // Check if process has exited
      if (subprocess.proc.exitCode !== null) {
        clearInterval(stdinPollInterval);
        return;
      }

      // Poll for stdin messages
      const message = popStdinMessage(commandName, projectDir);
      if (message) {
        if (VERY_VERBOSE_LOGS) {
          debugLog('[startService] writing stdin message: ' + message.data);
        }
        try {
          subprocess.proc.stdin?.write(message.data, message.encoding as BufferEncoding);
        } catch (error) {
          debugLog('[startService] error writing to stdin: ' + error);
        }
      }
    }, STDIN_POLL_INTERVAL_MS);

    // Clean up stdin polling when process exits
    subprocess.waitForExit().then(() => {
      clearInterval(stdinPollInterval);
    });
  }

  return subprocess;
}
