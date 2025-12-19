import { findConfigFile } from './configFile.ts';
import { findAllProcesses, findProcessesByProjectDir } from './database/processTable.ts';

export interface ListOutput {
  processes: {
    command: string;
    workingDir: string;
    uptime: string;
    pid: number;
    status: string;
    serviceName: string;
  }[];
  showAll?: boolean;
  message?: string;
}

export async function handleList(options?: { showAll?: boolean }): Promise<ListOutput> {
  const { config, projectDir } = findConfigFile(process.cwd());

  if (options?.showAll) {
    const processEntries = findAllProcesses();

    const processes = processEntries.map(processEntry => {
      return {
        serviceName: processEntry.command_name,
        command: processEntry.command_name,
        workingDir: processEntry.project_dir,
        uptime: formatUptime(Date.now() - processEntry.start_time * 1000),
        pid: processEntry.pid,
        status: 'RUNNING',
      };
    });
    return { processes };
  } else {
    const processEntries = findProcessesByProjectDir(projectDir);
    const runningByName = new Map(processEntries.map(p => [p.command_name, p]));
    const seenNames = new Set<string>();

    const processes: ListOutput['processes'] = [];

    // First, add all services from the config file
    for (const service of config.services || []) {
      seenNames.add(service.name);
      const runningProcess = runningByName.get(service.name);

      if (runningProcess) {
        processes.push({
          serviceName: service.name,
          command: service.name,
          workingDir: projectDir,
          uptime: formatUptime(Date.now() - runningProcess.start_time * 1000),
          pid: runningProcess.pid,
          status: 'RUNNING',
        });
      } else {
        processes.push({
          serviceName: service.name,
          command: service.name,
          workingDir: projectDir,
          uptime: '-',
          pid: 0,
          status: 'NOT LAUNCHED',
        });
      }
    }

    // Add any running processes from DB that aren't in the config (edge case)
    for (const processEntry of processEntries) {
      if (!seenNames.has(processEntry.command_name)) {
        processes.push({
          serviceName: processEntry.command_name,
          command: processEntry.command_name,
          workingDir: processEntry.project_dir,
          uptime: formatUptime(Date.now() - processEntry.start_time * 1000),
          pid: processEntry.pid,
          status: 'RUNNING',
        });
      }
    }

    return { processes };
  }
}

function formatUptime(milliseconds: number): string {
  const totalSeconds = Math.floor(milliseconds / 1000);
  const days = Math.floor(totalSeconds / 86400);
  const hours = Math.floor((totalSeconds % 86400) / 3600);
  const minutes = Math.floor((totalSeconds % 3600) / 60);
  const secs = totalSeconds % 60;

  const parts = [];
  if (days > 0) parts.push(`${days}d`);
  if (hours > 0) parts.push(`${hours}h`);
  if (minutes > 0) parts.push(`${minutes}m`);
  if (secs > 0 || parts.length === 0) parts.push(`${secs}s`);

  return parts.join(' ');
}

export function printListOutput(output: ListOutput): void {
  if (output.message) {
    console.log(output.message);
    return;
  }

  if (output.processes.length === 0) {
    console.log('No services configured.');
    return;
  }

  const headers = ['NAME', 'STATUS', 'PID', 'UPTIME', 'COMMAND', 'DIRECTORY'];
  const rows = output.processes.map(process => [
    process.serviceName,
    process.status,
    process.pid > 0 ? process.pid.toString() : '-',
    process.uptime,
    process.command,
    process.workingDir,
  ]);

  const columnWidths = headers.map((header, i) =>
    Math.max(header.length, ...rows.map(row => row[i].length))
  );

  const formatRow = (row: string[]) =>
    row.map((cell, i) => cell.padEnd(columnWidths[i])).join('  ');

  console.log(formatRow(headers));
  console.log(columnWidths.map(width => '-'.repeat(width)).join('  '));

  for (const row of rows) {
    console.log(formatRow(row));
  }
}
