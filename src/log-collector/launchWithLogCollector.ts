import { startShellCommand } from '@facetlayer/subprocess-wrapper';
import * as Path from 'node:path';
import * as fs from 'node:fs';
import { ProjectRootDir } from '../dirs.ts';
import type { LogCollectorLaunchInfo } from './LogCollectorLaunchInfo.ts';

function getRustCollectorPath(): string | null {
  const path = Path.join(ProjectRootDir, 'rust', 'target', 'release', 'candle-log-collector');
  if (fs.existsSync(path)) {
    return path;
  }
  return null;
}

export async function launchWithLogCollector(
  launchInfo: LogCollectorLaunchInfo,
  options?: { logCollector?: 'node' | 'rust' },
) {
  const useRust = options?.logCollector === 'rust';

  if (useRust) {
    const rustPath = getRustCollectorPath();
    if (!rustPath) {
      throw new Error(
        'Rust log collector not found. Build it first: cd rust && cargo build --release'
      );
    }
    return launchRustCollector(rustPath, launchInfo);
  }

  return launchNodeCollector(launchInfo);
}

async function launchNodeCollector(launchInfo: LogCollectorLaunchInfo) {
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

async function launchRustCollector(
  rustPath: string,
  launchInfo: LogCollectorLaunchInfo,
) {
  const subprocess = startShellCommand(rustPath, [], {
    stdio: ['pipe', 'pipe', 'pipe'],
    detached: true,
    onStdout: line => {
      //console.log('rust-log-collector stdout: ', line);
    },
    onStderr: line => {
      //console.log('rust-log-collector stderr: ', line);
    },
  });

  await subprocess.waitForStart();

  subprocess.proc.stdin.write(JSON.stringify(launchInfo));

  subprocess.proc.stdin.end();
  return subprocess;
}
