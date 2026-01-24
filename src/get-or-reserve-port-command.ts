import { getReservedPortForService, reservePort } from './port-assignment/index.ts';

export interface GetOrReservePortOptions {
  projectDir: string;
  serviceName: string;
}

export interface GetOrReservePortOutput {
  port: number;
  serviceName: string;
  projectDir: string;
  wasReserved: boolean;
}

export async function handleGetOrReservePort(
  options: GetOrReservePortOptions
): Promise<GetOrReservePortOutput> {
  const { projectDir, serviceName } = options;

  const existingPort = getReservedPortForService(projectDir, serviceName);
  if (existingPort) {
    return {
      port: existingPort.port,
      serviceName: existingPort.service_name!,
      projectDir: existingPort.project_dir,
      wasReserved: false,
    };
  }

  const port = await reservePort({ projectDir, serviceName });

  return {
    port,
    serviceName,
    projectDir,
    wasReserved: true,
  };
}

export function printGetOrReservePortOutput(output: GetOrReservePortOutput): void {
  console.log(output.port);
}
