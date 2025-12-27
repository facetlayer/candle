import { findConfigFile, type ServiceConfig } from './configFile.ts';
import {
  findAllProcesses,
  findProcessesByProjectDir,
  type ProcessEntry,
} from './database/processTable.ts';

export interface ListOutput {
  processes: {
    command: string;
    workingDir: string;
    uptime: string;
    pid: number;
    status: string;
    serviceName: string;
    configChanged?: boolean;
  }[];
  showAll?: boolean;
  message?: string;
}

function hasConfigDrift(
  processEntry: ProcessEntry,
  configService: ServiceConfig | undefined
): boolean {
  if (!configService) {
    // No config entry - this is a pure transient process, no drift to detect
    return false;
  }

  // Compare shell
  if (processEntry.shell !== configService.shell) {
    return true;
  }

  // Compare root (normalize undefined/null/"" to be equivalent)
  const dbRoot = processEntry.root || undefined;
  const configRoot = configService.root || undefined;
  if (dbRoot !== configRoot) {
    return true;
  }

  return false;
}

export async function handleList(options?: { showAll?: boolean }): Promise<ListOutput> {
  const { config, projectDir } = findConfigFile(process.cwd());

  // Build a map of service names to config entries for drift detection
  const configByName = new Map(
    (config.services || []).map(s => [s.name, s] as [string, ServiceConfig])
  );

  if (options?.showAll) {
    const processEntries = findAllProcesses();

    const processes = processEntries.map(processEntry => {
      const configService = configByName.get(processEntry.command_name);
      return {
        serviceName: processEntry.command_name,
        command: processEntry.command_name,
        workingDir: processEntry.project_dir,
        uptime: formatUptime(Date.now() - processEntry.start_time * 1000),
        pid: processEntry.pid,
        status: 'RUNNING',
        configChanged: hasConfigDrift(processEntry, configService),
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
          configChanged: hasConfigDrift(runningProcess, service),
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

    // Add any running processes from DB that aren't in the config (transient or orphaned)
    for (const processEntry of processEntries) {
      if (!seenNames.has(processEntry.command_name)) {
        const configService = configByName.get(processEntry.command_name);
        processes.push({
          serviceName: processEntry.command_name,
          command: processEntry.command_name,
          workingDir: processEntry.project_dir,
          uptime: formatUptime(Date.now() - processEntry.start_time * 1000),
          pid: processEntry.pid,
          status: 'RUNNING',
          configChanged: hasConfigDrift(processEntry, configService),
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
  const rows = output.processes.map(process => {
    // Add config changed indicator to status if applicable
    let status = process.status;
    if (process.configChanged) {
      status = `${status} [config changed]`;
    }

    return [
      process.serviceName,
      status,
      process.pid > 0 ? process.pid.toString() : '-',
      process.uptime,
      process.command,
      process.workingDir,
    ];
  });

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
