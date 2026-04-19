import * as fs from 'fs';
import * as path from 'path';
import { findConfigFile, readConfigFile, validateConfig } from './configFile.ts';
import { UsageError } from './errors.ts';

const VALID_CONFIG_KEYS: Record<string, { description: string; validate: (value: string) => any }> = {
  'logCollector': {
    description: "'node' or 'rust'",
    validate: (value: string) => {
      if (value !== 'node' && value !== 'rust') {
        throw new UsageError(`Invalid value for 'logCollector': expected 'node' or 'rust', got '${value}'`);
      }
      return value;
    },
  },
  'logEviction.maxLogsPerService': {
    description: 'positive integer',
    validate: (value: string) => {
      const num = Number(value);
      if (!Number.isInteger(num) || num < 1) {
        throw new UsageError(`Invalid value for 'logEviction.maxLogsPerService': expected a positive integer`);
      }
      return num;
    },
  },
  'logEviction.maxRetentionSeconds': {
    description: 'positive integer (seconds)',
    validate: (value: string) => {
      const num = Number(value);
      if (!Number.isInteger(num) || num < 1) {
        throw new UsageError(`Invalid value for 'logEviction.maxRetentionSeconds': expected a positive integer`);
      }
      return num;
    },
  },
};

export function handleSetConfig(key: string, value: string): void {
  const validator = VALID_CONFIG_KEYS[key];
  if (!validator) {
    const validKeys = Object.keys(VALID_CONFIG_KEYS).join(', ');
    throw new UsageError(`Unknown config key '${key}'. Valid keys: ${validKeys}`);
  }

  const parsedValue = validator.validate(value);

  const found = findConfigFile(process.cwd());
  const configPath = path.join(found.projectDir, found.configFilename);
  const config = readConfigFile(configPath);

  // Set the value using dot-notation key
  const parts = key.split('.');
  if (parts.length === 1) {
    (config as any)[parts[0]] = parsedValue;
  } else if (parts.length === 2) {
    if (!(config as any)[parts[0]]) {
      (config as any)[parts[0]] = {};
    }
    (config as any)[parts[0]][parts[1]] = parsedValue;
  }

  validateConfig(config);

  fs.writeFileSync(configPath, JSON.stringify(config, null, 2));

  console.log(`Set '${key}' to '${value}' in ${found.configFilename}`);
}
