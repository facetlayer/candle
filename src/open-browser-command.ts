import { spawn } from 'child_process';
import { platform } from 'os';
import { handleListPorts } from './list-ports-command.ts';
import { UsageError } from './errors.ts';
import { findProcessesByCommandNameAndProjectDir, findProcessesByProjectDir } from './database/processTable.ts';

export interface OpenBrowserOptions {
  projectDir: string;
  serviceName?: string;
}

export interface OpenBrowserOutput {
  serviceName: string;
  port: number;
  url: string;
}

function resolveServiceName(projectDir: string, providedName?: string): string {
  if (providedName) {
    return providedName;
  }

  const runningProcesses = findProcessesByProjectDir(projectDir);

  if (runningProcesses.length === 0) {
    throw new UsageError(
      'No service name provided and no running processes found in this project.'
    );
  }

  if (runningProcesses.length > 1) {
    const names = runningProcesses.map(p => p.command_name).join(', ');
    throw new UsageError(
      `No service name provided and multiple processes are running: ${names}. Please specify which service to open.`
    );
  }

  return runningProcesses[0].command_name;
}

export async function handleOpenBrowser(
  options: OpenBrowserOptions
): Promise<OpenBrowserOutput> {
  const { projectDir } = options;
  const serviceName = resolveServiceName(projectDir, options.serviceName);

  const portsOutput = await handleListPorts({ commandNames: [serviceName] });

  if (portsOutput.ports.length === 0) {
    const processes = findProcessesByCommandNameAndProjectDir(serviceName, projectDir);
    const isRunning = processes.some(p => p.killed_at === null);

    if (isRunning) {
      throw new UsageError(
        `No open ports found for service '${serviceName}'.`
      );
    } else {
      throw new UsageError(
        `No open ports found for service '${serviceName}'. Start the service with: candle start`
      );
    }
  }

  const sortedPorts = [...portsOutput.ports].sort((a, b) => a.port - b.port);
  const portInfo = sortedPorts[0];
  const url = `http://localhost:${portInfo.port}`;

  await openUrl(url);

  return {
    serviceName,
    port: portInfo.port,
    url,
  };
}

async function openUrl(url: string): Promise<void> {
  return new Promise((resolve, reject) => {
    let command: string;
    let args: string[];

    switch (platform()) {
      case 'darwin':
        command = 'open';
        args = [url];
        break;
      case 'win32':
        command = 'cmd';
        args = ['/c', 'start', '', url];
        break;
      default:
        command = 'xdg-open';
        args = [url];
        break;
    }

    const child = spawn(command, args, {
      stdio: 'ignore',
      detached: true,
    });

    child.on('error', (err) => {
      reject(new Error(`Failed to open browser: ${err.message}`));
    });

    child.on('spawn', () => {
      child.unref();
      resolve();
    });
  });
}

export function printOpenBrowserOutput(output: OpenBrowserOutput): void {
  console.log(`Opened ${output.url} in browser`);
}
