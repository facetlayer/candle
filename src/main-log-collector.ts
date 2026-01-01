import { startShellCommand, Subprocess } from '@facetlayer/subprocess-wrapper';
import * as Path from 'node:path';
import { maybeRunCleanup } from './database/cleanup.ts';
import { createProcessEntry, deleteProcessEntry } from './database/processTable.ts';
import type { LogCollectorLaunchInfo } from './log-collector/LogCollectorLaunchInfo.ts';
import { readStdinAsJson } from './log-collector/readStdinJson.ts';
import { ProcessLogType, saveProcessLog } from './logs/processLogs.ts';
import { debugLog } from './debug.ts';

const DEFAULT_GRACE_PERIOD_WAIT_MS = 500;

function startService(message: LogCollectorLaunchInfo): Subprocess {
  const { commandName, projectDir, shell, root } = message;

  let launchDir = projectDir;
  if (root) {
    launchDir = Path.join(projectDir, root);
  }

  return startShellCommand(shell, [], {
    shell: true,
    cwd: launchDir,
    onStdout: line => {
      saveProcessLog({
        command_name: commandName,
        project_dir: projectDir,
        content: line,
        log_type: ProcessLogType.stdout,
      });
    },
    onStderr: line => {
      saveProcessLog({
        command_name: commandName,
        project_dir: projectDir,
        content: line,
        log_type: ProcessLogType.stderr,
      });
    },
  });
}

async function main() {
  const launchInfo: LogCollectorLaunchInfo = await readStdinAsJson();

  // Check for cleanup on an interval.
  setInterval(maybeRunCleanup, 60 * 1000);

  debugLog('[main-log-collector] Got launchInfo: ' + JSON.stringify(launchInfo));

  const subprocess = startService(launchInfo);

  debugLog('[main-log-collector] Launched subprocess, pid=' + subprocess.proc.pid);

  createProcessEntry({
    commandName: launchInfo.commandName,
    projectDir: launchInfo.projectDir,
    pid: subprocess.proc.pid,
    logCollectorPid: process.pid,
    shell: launchInfo.shell,
    root: launchInfo.root,
  });

  try {
    await subprocess.waitForStart();
  } catch (error) {
    debugLog('[main-log-collector] Process failed to start, pid=' + subprocess.proc.pid + ', error=' + error.message);
    saveProcessLog({
      command_name: launchInfo.commandName,
      project_dir: launchInfo.projectDir,
      log_type: ProcessLogType.process_start_failed,
      content: 'Process failed to start: ' + error.message,
    });
    process.exit(1);
  }
  
  // Grace period: Wait for a short period to ensure the process does not fail quickly.
  await new Promise(resolve => setTimeout(resolve, DEFAULT_GRACE_PERIOD_WAIT_MS));

  if (subprocess.proc.exitCode != null && subprocess.proc.exitCode !== 0) {
    debugLog('[main-log-collector] Process failed during grace period, pid=' + subprocess.proc.pid + ', code=' + subprocess.proc.exitCode);
    saveProcessLog({
      command_name: launchInfo.commandName,
      project_dir: launchInfo.projectDir,
      log_type: ProcessLogType.process_start_failed,
      content: 'Process failed to start: ' + subprocess.proc.exitCode,
    });
    deleteProcessEntry({
      commandName: launchInfo.commandName,
      projectDir: launchInfo.projectDir,
      pid: subprocess.proc.pid,
    });
    return;
  }

  debugLog('[main-log-collector] Process started, pid=' + subprocess.proc.pid);

  saveProcessLog({
    command_name: launchInfo.commandName,
    project_dir: launchInfo.projectDir,
    log_type: ProcessLogType.process_started,
  });

  await subprocess.waitForExit();

  debugLog('[main-log-collector] Process exited, pid=' + subprocess.proc.pid + ', code=' + subprocess.proc.exitCode);

  saveProcessLog({
    command_name: launchInfo.commandName,
    project_dir: launchInfo.projectDir,
    log_type: ProcessLogType.process_exited,
    content: 'Process exited with code ' + subprocess.proc.exitCode,
  });

  deleteProcessEntry({
    commandName: launchInfo.commandName,
    projectDir: launchInfo.projectDir,
    pid: subprocess.proc.pid,
  });
}

main().catch(error => {
  console.error(error);
  process.exit(1);
});
