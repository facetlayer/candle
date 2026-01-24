import {
  releasePortsForProject,
  releasePortForService,
  getReservedPortForService,
} from './port-assignment/index.ts';
import { UsageError } from './errors.ts';

export interface ReleasePortsOptions {
  projectDir: string;
  serviceName?: string;
}

export interface ReleasePortsOutput {
  releasedCount: number;
  projectDir: string;
  serviceName?: string;
  releasedPort?: number;
}

export async function handleReleasePorts(options: ReleasePortsOptions): Promise<ReleasePortsOutput> {
  const { projectDir, serviceName } = options;

  if (serviceName) {
    // Release port for a specific service
    const existingPort = getReservedPortForService(projectDir, serviceName);
    const released = releasePortForService(projectDir, serviceName);

    if (!released) {
      throw new UsageError(`No reserved port found for service '${serviceName}'`);
    }

    return {
      releasedCount: 1,
      projectDir,
      serviceName,
      releasedPort: existingPort?.port,
    };
  } else {
    // Release all ports for the project
    const releasedCount = releasePortsForProject(projectDir);

    return {
      releasedCount,
      projectDir,
    };
  }
}

export function printReleasePortsOutput(output: ReleasePortsOutput): void {
  if (output.serviceName) {
    console.log(`Released port ${output.releasedPort} for service '${output.serviceName}'`);
  } else if (output.releasedCount === 0) {
    console.log('No reserved ports to release');
  } else if (output.releasedCount === 1) {
    console.log('Released 1 reserved port');
  } else {
    console.log(`Released ${output.releasedCount} reserved ports`);
  }
}
