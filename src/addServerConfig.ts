import * as fs from 'fs';
import * as path from 'path';
import type { CandleSetupConfig, ServiceConfig } from './configFile.ts';
import { DEFAULT_CONFIG_FILENAME, findConfigFile, validateConfig } from './configFile.ts';
import { MissingSetupFileError } from './errors.ts';

export interface AddServerConfigArgs {
  name: string;
  shell: string;
  root?: string;
  pty?: boolean;
}

export function addServerConfig(args: AddServerConfigArgs, startDir: string = process.cwd()): void {
  const configPath = findOrCreateSetupFile(startDir);

  // Read existing config
  const content = fs.readFileSync(configPath, 'utf8');
  const config = validateConfig(JSON.parse(content) as CandleSetupConfig);

  // Check if service name already exists
  if (config.services.some(service => service.name === args.name)) {
    throw new Error(`Service '${args.name}' already exists in configuration`);
  }

  // Create new service config
  const newService: ServiceConfig = {
    name: args.name,
    shell: args.shell,
    ...(args.root && { root: args.root }),
    ...(args.pty && { pty: args.pty }),
  };

  // Add new service to config
  config.services.push(newService);

  // Validate the updated config
  validateConfig(config);

  // Write back to file
  fs.writeFileSync(configPath, JSON.stringify(config, null, 2));

  console.log(`Service '${args.name}' added successfully to .candle.json`);
}

function findOrCreateSetupFile(startDir: string): string {
  try {
    const setupResult = findConfigFile(startDir);
    return path.join(setupResult.projectDir, setupResult.configFilename);
  } catch (error) {
    if (!(error instanceof MissingSetupFileError)) {
      throw error;
    }
  }

  // Create new config file in the current directory (using the default filename)
  const configPath = path.join(startDir, DEFAULT_CONFIG_FILENAME);
  const initialConfig: CandleSetupConfig = {
    services: [],
  };

  fs.writeFileSync(configPath, JSON.stringify(initialConfig, null, 2));
  return configPath;
}
