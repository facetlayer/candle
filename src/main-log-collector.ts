#! /usr/bin/env node

import yargs from 'yargs';
import { hideBin } from 'yargs/helpers';
import { maybeRunCleanup } from './database/cleanup.ts';
import { createProcessEntry, deleteProcessEntry } from './database/processTable.ts';
import type { LogCollectorLaunchInfo } from './log-collector/LogCollectorLaunchInfo.ts';
import { readStdinAsJson } from './log-collector/readStdinJson.ts';
import { startMonitoredService } from './log-collector/startMonitoredService.ts';
import { saveProcessLog } from './logs/processLogs.ts';
import { ProcessLogType } from './logs/ProcessLogType.ts';
import { debugLog } from './debug.ts';
import * as Path from 'node:path';

const DEFAULT_GRACE_PERIOD_WAIT_MS = 500;

async function getLaunchInfo(): Promise<LogCollectorLaunchInfo> {
  const args = hideBin(process.argv);

  if (args.length === 0) {
    return readStdinAsJson();
  }

  const parsed = await yargs(args)
    .option('commandName', {
      type: 'string',
      demandOption: true,
      description: 'Name of the command/service',
    })
    .option('projectDir', {
      type: 'string',
      demandOption: true,
      description: 'Project directory',
    })
    .option('shell', {
      type: 'string',
      demandOption: true,
      description: 'Shell command to run',
    })
    .option('root', {
      type: 'string',
      description: 'Root directory relative to projectDir',
    })
    .option('pty', {
      type: 'boolean',
      default: false,
      description: 'Use PTY for interactive process',
    })
    .option('enableStdin', {
      type: 'boolean',
      default: false,
      description: 'Enable stdin message polling from database',
    })
    .parse();

  return {
    commandName: parsed.commandName,
    projectDir: Path.resolve(parsed.projectDir),
    shell: parsed.shell,
    root: parsed.root,
    pty: parsed.pty,
    enableStdin: parsed.enableStdin,
  };
}

async function main() {
  const launchInfo = await getLaunchInfo();

  // Check for cleanup on an interval.
  setInterval(maybeRunCleanup, 60 * 1000);

  debugLog('[main-log-collector] Got launchInfo: ' + JSON.stringify(launchInfo));

  const subprocess = startMonitoredService(launchInfo);

  debugLog('[main-log-collector] Launched subprocess, pid=' + subprocess.pid);

  createProcessEntry({
    commandName: launchInfo.commandName,
    projectDir: launchInfo.projectDir,
    pid: subprocess.pid,
    logCollectorPid: process.pid,
    shell: launchInfo.shell,
    root: launchInfo.root,
  });

  try {
    await subprocess.waitForStart();
  } catch (error) {
    debugLog('[main-log-collector] Process failed to start, pid=' + subprocess.pid + ', error=' + error.message);
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

  if (subprocess.getExitCode() != null && subprocess.getExitCode() !== 0) {
    debugLog('[main-log-collector] Process failed during grace period, pid=' + subprocess.pid + ', code=' + subprocess.getExitCode());
    saveProcessLog({
      command_name: launchInfo.commandName,
      project_dir: launchInfo.projectDir,
      log_type: ProcessLogType.process_start_failed,
      content: 'Process failed to start: ' + subprocess.getExitCode(),
    });
    deleteProcessEntry({
      commandName: launchInfo.commandName,
      projectDir: launchInfo.projectDir,
      pid: subprocess.pid,
    });
    return;
  }

  debugLog('[main-log-collector] Process started, pid=' + subprocess.pid);

  saveProcessLog({
    command_name: launchInfo.commandName,
    project_dir: launchInfo.projectDir,
    log_type: ProcessLogType.process_started,
  });

  await subprocess.waitForExit();

  debugLog('[main-log-collector] Process exited, pid=' + subprocess.pid + ', code=' + subprocess.getExitCode());

  saveProcessLog({
    command_name: launchInfo.commandName,
    project_dir: launchInfo.projectDir,
    log_type: ProcessLogType.process_exited,
    content: 'Process exited with code ' + subprocess.getExitCode(),
  });

  deleteProcessEntry({
    commandName: launchInfo.commandName,
    projectDir: launchInfo.projectDir,
    pid: subprocess.pid,
  });
}

main().catch(error => {
  console.error(error);
  process.exit(1);
});
