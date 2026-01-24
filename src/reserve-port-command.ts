import { reservePort } from './port-assignment/index.ts';

export interface ReservePortOptions {
  projectDir: string;
  serviceName?: string;
}

export interface ReservePortOutput {
  port: number;
  projectDir: string;
  serviceName?: string;
}

export async function handleReservePort(options: ReservePortOptions): Promise<ReservePortOutput> {
  const { projectDir, serviceName } = options;

  const port = await reservePort({
    projectDir,
    serviceName,
  });

  return {
    port,
    projectDir,
    serviceName,
  };
}

export function printReservePortOutput(output: ReservePortOutput): void {
  if (output.serviceName) {
    console.log(`Reserved port ${output.port} for service '${output.serviceName}'`);
  } else {
    console.log(`Reserved port ${output.port} for project`);
  }
}
