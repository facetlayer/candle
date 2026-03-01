import * as fs from 'fs';
import * as path from 'path';
import type { CandleSetupConfig } from './configFile.ts';
import { DEFAULT_CONFIG_FILENAME, findConfigFile } from './configFile.ts';
import { MissingSetupFileError } from './errors.ts';

export function handleSetupProject(startDir: string = process.cwd()): void {
  try {
    const result = findConfigFile(startDir);
    const configPath = path.join(result.projectDir, result.configFilename);
    console.log(`Config file already exists at ${configPath}`);
    return;
  } catch (error) {
    if (!(error instanceof MissingSetupFileError)) {
      throw error;
    }
  }

  const configPath = path.join(startDir, DEFAULT_CONFIG_FILENAME);
  const initialConfig: CandleSetupConfig = {
    services: [],
  };

  fs.writeFileSync(configPath, JSON.stringify(initialConfig, null, 2));
  console.log(`Created ${DEFAULT_CONFIG_FILENAME} in ${startDir}`);
}
