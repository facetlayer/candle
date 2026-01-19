import { spawn } from 'child_process';
import { findConfigFile } from './configFile.ts';
import {
  findAllProcesses,
  findProcessesByProjectDir,
  type ProcessEntry,
} from './database/processTable.ts';
import { getProcessTree } from './process-tree.ts';

export interface PortInfo {
  serviceName: string;
  pid: number;
  port: number;
  address: string;
  protocol: string;
  isChildProcess: boolean;
}

export interface ListPortsOutput {
  ports: PortInfo[];
}

export async function handleListPorts(options?: { showAll?: boolean }): Promise<ListPortsOutput> {
  const { projectDir } = findConfigFile(process.cwd());

  const processEntries = options?.showAll
    ? findAllProcesses()
    : findProcessesByProjectDir(projectDir);

  const allPorts: PortInfo[] = [];

  for (const processEntry of processEntries) {
    const ports = await getPortsForService(processEntry);
    allPorts.push(...ports);
  }

  return { ports: allPorts };
}

async function getPortsForService(processEntry: ProcessEntry): Promise<PortInfo[]> {
  const rootPid = processEntry.pid;
  const serviceName = processEntry.command_name;

  // Get all PIDs in the process tree
  const allPids = await getProcessTree(rootPid);

  if (allPids.length === 0) {
    return [];
  }

  // Get listening ports for all PIDs
  const portInfos = await getListeningPorts(allPids);

  // Map to PortInfo with service name
  return portInfos.map((info) => ({
    serviceName,
    pid: info.pid,
    port: info.port,
    address: info.address,
    protocol: info.protocol,
    isChildProcess: info.pid !== rootPid,
  }));
}

interface RawPortInfo {
  pid: number;
  port: number;
  address: string;
  protocol: string;
}

async function getListeningPorts(pids: number[]): Promise<RawPortInfo[]> {
  if (pids.length === 0) {
    return [];
  }

  const pidSet = new Set(pids);

  // Get all listening TCP ports, then filter by our PIDs
  // Using -iTCP -sTCP:LISTEN is more reliable than -p PID -i on macOS
  return new Promise((resolve) => {
    const child = spawn('lsof', ['-iTCP', '-sTCP:LISTEN', '-n', '-P'], {
      stdio: ['ignore', 'pipe', 'ignore'],
    });

    let stdout = '';
    child.stdout.on('data', (data) => {
      stdout += data.toString();
    });

    child.on('close', () => {
      const allPorts = parseLsofOutput(stdout);
      // Filter to only include ports from our PIDs
      const filteredPorts = allPorts.filter((port) => pidSet.has(port.pid));
      resolve(filteredPorts);
    });

    child.on('error', () => {
      // lsof not available or error
      resolve([]);
    });
  });
}

/**
 * Parse lsof output to extract listening port information.
 *
 * Example lsof output:
 * COMMAND   PID   USER   FD   TYPE DEVICE SIZE/OFF NODE NAME
 * node    12345   user   45u  IPv4 0x1234    0t0  TCP 127.0.0.1:3000 (LISTEN)
 * node    12345   user   46u  IPv4 0x1235    0t0  TCP *:8080 (LISTEN)
 */
function parseLsofOutput(output: string): RawPortInfo[] {
  const lines = output.split('\n');
  const portInfos: RawPortInfo[] = [];

  for (const line of lines) {
    // Only process lines with LISTEN
    if (!line.includes('LISTEN')) {
      continue;
    }

    const parts = line.split(/\s+/);
    if (parts.length < 9) {
      continue;
    }

    // Extract PID (second column)
    const pid = parseInt(parts[1], 10);
    if (isNaN(pid)) {
      continue;
    }

    // Extract protocol from NODE column (typically column 8, value is TCP or UDP)
    // The protocol is in the TYPE column (column 5) or we can infer from NODE
    // Actually, the NODE column shows TCP/UDP for network connections
    const nodeIndex = parts.findIndex((p) => p === 'TCP' || p === 'UDP');
    const protocol = nodeIndex >= 0 ? parts[nodeIndex] : 'TCP';

    // Extract NAME column (last column, format: address:port (LISTEN))
    const nameColumn = parts[parts.length - 2]; // Second to last, as last is "(LISTEN)"
    if (!nameColumn || !nameColumn.includes(':')) {
      continue;
    }

    // Parse address:port
    const lastColonIndex = nameColumn.lastIndexOf(':');
    const address = nameColumn.substring(0, lastColonIndex);
    const portStr = nameColumn.substring(lastColonIndex + 1);
    const port = parseInt(portStr, 10);

    if (isNaN(port)) {
      continue;
    }

    // Normalize address: * means 0.0.0.0 (or all interfaces)
    const normalizedAddress = address === '*' ? '0.0.0.0' : address;

    portInfos.push({
      pid,
      port,
      address: normalizedAddress,
      protocol,
    });
  }

  // Deduplicate by pid+port (lsof can show both IPv4 and IPv6 for same port)
  const seen = new Set<string>();
  return portInfos.filter((info) => {
    const key = `${info.pid}:${info.port}`;
    if (seen.has(key)) {
      return false;
    }
    seen.add(key);
    return true;
  });
}

export function printListPortsOutput(output: ListPortsOutput): void {
  if (output.ports.length === 0) {
    console.log('No open ports found for running services.');
    return;
  }

  const headers = ['SERVICE', 'PID', 'PORT', 'ADDRESS', 'PROTOCOL'];
  const rows = output.ports.map((portInfo) => {
    const suffix = portInfo.isChildProcess ? ' (child)' : '';
    return [
      portInfo.serviceName,
      portInfo.pid.toString(),
      portInfo.port.toString(),
      portInfo.address,
      portInfo.protocol + suffix,
    ];
  });

  const columnWidths = headers.map((header, i) =>
    Math.max(header.length, ...rows.map((row) => row[i].length))
  );

  const formatRow = (row: string[]) =>
    row.map((cell, i) => cell.padEnd(columnWidths[i])).join('  ');

  console.log(formatRow(headers));
  console.log(columnWidths.map((width) => '-'.repeat(width)).join('  '));

  for (const row of rows) {
    console.log(formatRow(row));
  }
}
