import { startShellCommand } from '@facetlayer/subprocess-wrapper';
import * as pty from 'node-pty';
import * as Path from 'node:path';
import type { LogCollectorLaunchInfo } from './LogCollectorLaunchInfo.ts';
import type { MonitoredProcess } from './MonitoredProcess.ts';
import { saveProcessLog } from '../logs/processLogs.ts';
import { ProcessLogType } from '../logs/ProcessLogType.ts';
import { debugLog } from '../debug.ts';
import { popStdinMessage, clearStdinMessages } from '../database/stdinMessagesTable.ts';

const VERY_VERBOSE_LOGS = true;

const STDIN_POLL_INTERVAL_MS = 500;

export function startMonitoredService(message: LogCollectorLaunchInfo): MonitoredProcess {
  const { commandName, projectDir, shell, root, pty, enableStdin } = message;

  let launchDir = projectDir;
  if (root) {
    launchDir = Path.join(projectDir, root);
  }

  if (VERY_VERBOSE_LOGS) {
    debugLog('[startService] starting shell command ' + JSON.stringify(shell, null, 2) + ' pty=' + pty + ' enableStdin=' + enableStdin);
  }

  if (pty) {
    // PTY mode does not support enableStdin
    return startWithPty(shell, launchDir, commandName, projectDir);
  } else {
    return startWithSubprocess(shell, launchDir, commandName, projectDir, enableStdin);
  }
}

function startWithPty(
  shell: string,
  launchDir: string,
  commandName: string,
  projectDir: string,
): MonitoredProcess {
  debugLog('[startWithPty] starting pty with shell: ' + shell);

  const ptyProcess = pty.spawn('/bin/bash', ['-c', shell], {
    name: 'xterm-color',
    cols: 80,
    rows: 30,
    cwd: launchDir,
    env: process.env as Record<string, string>,
  });

  let exitCode: number | null = null;
  let exitResolve: (() => void) | null = null;
  const exitPromise = new Promise<void>(resolve => {
    exitResolve = resolve;
  });

  ptyProcess.onData((data: string) => {
    saveProcessLog({
      command_name: commandName,
      project_dir: projectDir,
      content: data,
      log_type: ProcessLogType.stdout,
    });
    if (VERY_VERBOSE_LOGS) {
      debugLog('[startService] pty data: ' + data);
    }
  });

  ptyProcess.onExit(({ exitCode: code }) => {
    exitCode = code;
    if (exitResolve) {
      exitResolve();
    }
  });

  return {
    pid: ptyProcess.pid,
    getExitCode: () => exitCode,
    waitForStart: async () => {
      // PTY spawns immediately, no async startup
    },
    waitForExit: () => exitPromise,
  };
}

function startWithSubprocess(
  shell: string,
  launchDir: string,
  commandName: string,
  projectDir: string,
  enableStdin?: boolean,
): MonitoredProcess {
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
  let stdinPollInterval: NodeJS.Timeout | null = null;
  if (enableStdin) {
    // Clear any stale stdin messages from previous runs
    clearStdinMessages(commandName, projectDir);

    stdinPollInterval = setInterval(() => {
      // Check if process has exited
      if (subprocess.proc.exitCode !== null) {
        if (stdinPollInterval) {
          clearInterval(stdinPollInterval);
          stdinPollInterval = null;
        }
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
  }

  return {
    pid: subprocess.proc.pid!,
    getExitCode: () => subprocess.proc.exitCode,
    waitForStart: () => subprocess.waitForStart(),
    waitForExit: async () => {
      await subprocess.waitForExit();
      // Clean up stdin polling when process exits
      if (stdinPollInterval) {
        clearInterval(stdinPollInterval);
      }
    },
  };
}
