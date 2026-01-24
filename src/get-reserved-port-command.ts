import { getReservedPortForService } from './port-assignment/index.ts';
import { UsageError } from './errors.ts';

export interface GetReservedPortOptions {
  projectDir: string;
  serviceName: string;
}

export interface GetReservedPortOutput {
  port: number;
  serviceName: string;
  projectDir: string;
  assignedAt: number;
}

export async function handleGetReservedPort(
  options: GetReservedPortOptions
): Promise<GetReservedPortOutput> {
  const { projectDir, serviceName } = options;

  const reservedPort = getReservedPortForService(projectDir, serviceName);

  if (!reservedPort) {
    throw new UsageError(`No reserved port found for service '${serviceName}'`);
  }

  return {
    port: reservedPort.port,
    serviceName: reservedPort.service_name!,
    projectDir: reservedPort.project_dir,
    assignedAt: reservedPort.assigned_at,
  };
}

export function printGetReservedPortOutput(output: GetReservedPortOutput): void {
  console.log(output.port);
}
