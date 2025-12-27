import { startShellCommand } from '@facetlayer/subprocess-wrapper';
import * as Path from 'node:path';
import { ProjectRootDir } from '../dirs.ts';
import type { LogCollectorLaunchInfo } from './LogCollectorLaunchInfo.ts';

export async function launchWithLogCollector(launchInfo: LogCollectorLaunchInfo) {
  const command = process.argv[0];
  const args = [Path.join(ProjectRootDir, 'dist', 'main-log-collector.js')];

  const subprocess = startShellCommand(command, args, {
    stdio: ['pipe', 'pipe', 'pipe'],
    detached: true,
    onStdout: line => {
      //console.log('log-collector stdout: ', line);
    },
    onStderr: line => {
      //console.log('log-collector stderr: ', line);
    },
  });

  await subprocess.waitForStart();

  subprocess.proc.stdin.write(JSON.stringify(launchInfo));

  subprocess.proc.stdin.end();
  return subprocess;
}
