import { spawn } from 'child_process';
import { platform } from 'os';
import { handleListPorts } from './list-ports-command.ts';
import { UsageError } from './errors.ts';

export interface OpenBrowserOptions {
  serviceName: string;
}

export interface OpenBrowserOutput {
  serviceName: string;
  port: number;
  url: string;
}

export async function handleOpenBrowser(
  options: OpenBrowserOptions
): Promise<OpenBrowserOutput> {
  const { serviceName } = options;

  const portsOutput = await handleListPorts({ serviceName });

  if (portsOutput.ports.length === 0) {
    throw new UsageError(
      `No open ports found for service '${serviceName}'. Is the service running?`
    );
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
