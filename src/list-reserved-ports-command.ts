import {
  getAllReservedPorts,
  getReservedPortsForProject,
  type ReservedPort,
} from './port-assignment/index.ts';
import { findConfigFile } from './configFile.ts';

export interface ListReservedPortsOptions {
  showAll?: boolean;
}

export interface ListReservedPortsOutput {
  ports: ReservedPort[];
  showAll: boolean;
}

export async function handleListReservedPorts(
  options: ListReservedPortsOptions = {}
): Promise<ListReservedPortsOutput> {
  const { showAll = false } = options;

  let ports: ReservedPort[];

  if (showAll) {
    ports = getAllReservedPorts();
  } else {
    // Get ports for current project directory
    const configResult = findConfigFile(process.cwd());
    if (!configResult) {
      ports = [];
    } else {
      ports = getReservedPortsForProject(configResult.projectDir);
    }
  }

  return {
    ports,
    showAll,
  };
}

export function printListReservedPortsOutput(output: ListReservedPortsOutput): void {
  if (output.ports.length === 0) {
    if (output.showAll) {
      console.log('No reserved ports found');
    } else {
      console.log('No reserved ports found for current project');
    }
    return;
  }

  console.log('Reserved ports:');
  console.log('');

  for (const port of output.ports) {
    const servicePart = port.service_name ? ` (service: ${port.service_name})` : ' (project-level)';
    const datePart = new Date(port.assigned_at * 1000).toLocaleString();
    console.log(`  Port ${port.port}${servicePart}`);
    console.log(`    Project: ${port.project_dir}`);
    console.log(`    Reserved at: ${datePart}`);
    console.log('');
  }
}
