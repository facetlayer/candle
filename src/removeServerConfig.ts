import * as fs from 'fs';
import * as path from 'path';
import { findConfigFile, readConfigFile, validateConfig } from './configFile.ts';

export function removeServerConfig(name: string, startDir: string = process.cwd()): void {
  const setupResult = findConfigFile(startDir);
  const configPath = path.join(setupResult.projectDir, setupResult.configFilename);

  const config = readConfigFile(configPath);

  const originalLength = config.services.length;
  config.services = config.services.filter(service => service.name !== name);

  if (config.services.length === originalLength) {
    throw new Error(`Service '${name}' not found in configuration`);
  }

  validateConfig(config);

  fs.writeFileSync(configPath, JSON.stringify(config, null, 2));

  console.log(`Service '${name}' removed from .candle.json`);
}
