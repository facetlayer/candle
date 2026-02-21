import * as fs from 'fs';
import * as path from 'path';
import {
  findProcessesByCommandNameAndProjectDir,
  type ProcessEntry,
} from './database/processTable.ts';
import { ConfigFileError, MissingServiceWithNameError, MissingSetupFileError } from './errors.ts';

export interface ServiceConfig {
  name: string;
  shell: string;
  root?: string;
  enableStdin?: boolean;
}

export interface LogEvictionConfig {
  maxLogsPerService?: number;
  maxRetentionSeconds?: number;
}

export interface CandleSetupConfig {
  services?: ServiceConfig[];
  logEviction?: LogEvictionConfig;
}

// Config filenames in priority order (first match wins)
// .candle-setup.json is deprecated but still supported for backwards compatibility
export const CONFIG_FILENAMES = ['.candle.json', '.candle-setup.json'] as const;
export const DEFAULT_CONFIG_FILENAME = '.candle.json';

/*
  findProjectDir

  Finds the location of the nearest config file (.candle.json or .candle-setup.json).
*/
export function findProjectDir(cwd: string = process.cwd()): string {
  const setupResult = findConfigFile(cwd);
  if (setupResult) {
    return setupResult.projectDir;
  }

  throw new MissingSetupFileError(cwd);
}

/**
 * Reads and parses a config file.
 */
export function readConfigFile(configFilePath: string): CandleSetupConfig {
  const content = fs.readFileSync(configFilePath, 'utf8').trim();

  if (content.length === 0) {
    // Empty file is allowed.
    return { services: [] };
  }

  const config = JSON.parse(content) as CandleSetupConfig;

  // Make sure .services is not undefined.
  config.services = config.services || [];

  return validateConfig(config);
}

/**
 * Finds the nearest config file in the target directory or above.
 */
export function findConfigFile(
  currentDir: string
): { config: CandleSetupConfig; projectDir: string; configFilename: string } | null {
  const startingDir = currentDir;
  currentDir = currentDir ? path.resolve(currentDir) : process.cwd();

  while (true) {
    // Check each config filename in priority order
    for (const filename of CONFIG_FILENAMES) {
      const configFilePath = path.join(currentDir, filename);

      if (fs.existsSync(configFilePath)) {
        try {
          const config = readConfigFile(configFilePath);
          return { config, projectDir: currentDir, configFilename: filename };
        } catch (error) {
          throw new Error(`Invalid ${filename} at ${configFilePath}: ${error.message}`);
        }
      }
    }

    const parentDir = path.dirname(currentDir);
    if (parentDir === currentDir) {
      // Reached root directory
      break;
    }
    currentDir = parentDir;
  }

  throw new MissingSetupFileError(startingDir);
}

/**
 * Validate & typecheck the config file contents.
 */
export function validateConfig(config: CandleSetupConfig) {
  const names = new Set<string>();

  let services = config.services || [];

  if (!Array.isArray(services)) {
    // Handle a situation where 'services' is an object instead of array.
    if (typeof services === 'object' && services !== null) {
      let fixedServices: ServiceConfig[] = [];
      for (const [key, value] of Object.entries(services)) {
        fixedServices.push({
          name: key,
          ...(value as ServiceConfig),
        });
      }
      services = fixedServices;
    } else {
      throw new ConfigFileError(
        `Config file error: Invalid value for 'services': ${JSON.stringify(services)}`
      );
    }
  }

  for (const service of services) {
    if (!service.name || typeof service.name !== 'string') {
      throw new ConfigFileError('Config file error: Each service must have a "name" string');
    }

    if (!service.shell || typeof service.shell !== 'string') {
      throw new ConfigFileError(
        `Config file error: Service "${service.name}" must have a "shell" string`
      );
    }

    if (names.has(service.name)) {
      throw new ConfigFileError(`Config file error: Duplicate service name: "${service.name}"`);
    }
    names.add(service.name);

    if (service.root && !isValidRelativePath(service.root)) {
      throw new ConfigFileError(
        `Service "${service.name}" has invalid root path: "${service.root}"`
      );
    }
  }

  // Validate logEviction config if present
  if (config.logEviction !== undefined) {
    if (typeof config.logEviction !== 'object' || config.logEviction === null || Array.isArray(config.logEviction)) {
      throw new ConfigFileError(
        `Config file error: Invalid value for 'logEviction': expected an object`
      );
    }

    const { maxLogsPerService, maxRetentionSeconds } = config.logEviction;

    if (maxLogsPerService !== undefined) {
      if (typeof maxLogsPerService !== 'number' || !Number.isInteger(maxLogsPerService) || maxLogsPerService < 1) {
        throw new ConfigFileError(
          `Config file error: 'logEviction.maxLogsPerService' must be a positive integer`
        );
      }
    }

    if (maxRetentionSeconds !== undefined) {
      if (typeof maxRetentionSeconds !== 'number' || !Number.isInteger(maxRetentionSeconds) || maxRetentionSeconds < 1) {
        throw new ConfigFileError(
          `Config file error: 'logEviction.maxRetentionSeconds' must be a positive integer`
        );
      }
    }
  }

  return {
    ...config,
    services,
  };
}


export function isValidRelativePath(p: string): boolean {
  // Don't allow absolute paths or paths that escape the config directory
  if (path.isAbsolute(p)) {
    return false;
  }

  const normalized = path.normalize(p);
  if (normalized.startsWith('..')) {
    return false;
  }

  return true;
}

export const LOG_EVICTION_DEFAULTS = {
  maxLogsPerService: 1000,
  maxRetentionSeconds: 24 * 60 * 60,
} as const;

export interface ResolvedLogEvictionConfig {
  maxLogsPerService: number;
  maxRetentionSeconds: number;
}

export function getLogEvictionConfig(config?: CandleSetupConfig): ResolvedLogEvictionConfig {
  return {
    maxLogsPerService: config?.logEviction?.maxLogsPerService ?? LOG_EVICTION_DEFAULTS.maxLogsPerService,
    maxRetentionSeconds: config?.logEviction?.maxRetentionSeconds ?? LOG_EVICTION_DEFAULTS.maxRetentionSeconds,
  };
}

export function getServiceCwd(service: ServiceConfig, configPath: string): string {
  const configDir = path.dirname(configPath);
  if (service.root) {
    return path.resolve(configDir, service.root);
  }
  return configDir;
}

export function findServiceByName(config: CandleSetupConfig, name: string): ServiceConfig | null {
  return config.services.find(s => s.name === name) || null;
}

export function getAllServiceNames(config: CandleSetupConfig): string[] {
  return config.services.map(s => s.name);
}

interface FoundServiceConfig {
  serviceConfig: ServiceConfig;
  projectDir: string;
}

function findLooseCommandName(
  commandName: string,
  config: CandleSetupConfig,
  projectDir: string,
  currentDir?: string
): ServiceConfig | null {
  const searchDir = currentDir || process.cwd();

  // Find all services where commandName is a substring of the service name
  const matchingServices = config.services.filter(service => service.name.includes(commandName));

  if (matchingServices.length === 0) {
    // No matches found, try parent directory
    const parentDir = path.dirname(searchDir);
    if (parentDir === searchDir || parentDir === projectDir) {
      // Reached root or project directory, give up
      return null;
    }
    return findLooseCommandName(commandName, config, projectDir, parentDir);
  }

  // Find services that match the current directory
  const servicesWithMatchingRoot = matchingServices.filter(service => {
    if (!service.root) {
      // Service has no root, matches project directory
      return searchDir === projectDir;
    }
    const serviceRootPath = path.resolve(projectDir, service.root);
    return searchDir === serviceRootPath;
  });

  if (servicesWithMatchingRoot.length === 1) {
    return servicesWithMatchingRoot[0];
  } else if (servicesWithMatchingRoot.length > 1) {
    const serviceNames = servicesWithMatchingRoot.map(s => s.name).join(', ');
    throw new Error(
      `Ambiguous service name "${commandName}". Multiple services match in current directory: ${serviceNames}`
    );
  }

  // No services match current directory, try parent directory
  const parentDir = path.dirname(searchDir);
  if (parentDir === searchDir || parentDir === projectDir) {
    // Reached root or project directory, return null if no exact match
    return matchingServices.length === 1 ? matchingServices[0] : null;
  }

  return findLooseCommandName(commandName, config, projectDir, parentDir);
}

export function getServiceConfigByName(
  commandName: string,
  currentDir?: string
): FoundServiceConfig {
  const foundConfig = findConfigFile(currentDir);

  const { config, projectDir } = foundConfig;

  let serviceConfig: ServiceConfig | null = config.services.find(s => s.name === commandName) || null;

  if (!serviceConfig) {
    // Try loose matching if exact match fails
    serviceConfig = findLooseCommandName(commandName, config, projectDir, currentDir);
  }

  if (!serviceConfig) {
    throw new MissingServiceWithNameError(projectDir, commandName);
  }

  return {
    serviceConfig,
    projectDir,
  };
}

/**
 * Result from getServiceInfoByName - provides info about a service whether
 * it's running (from DB) or just configured (from config file).
 */
export interface ServiceInfo {
  commandName: string;
  projectDir: string;
  /** The running process entry if the service is currently running */
  runningProcess?: ProcessEntry;
  /** The service config if defined in config file */
  serviceConfig?: ServiceConfig;
}

/**
 * Finds service info by name, checking both running processes (DB) and config file.
 *
 * Priority:
 * 1. If a process with this name is running in the current project, return DB info
 * 2. Otherwise, look up the service in the config file
 *
 * This supports both config-defined services and transient processes.
 */
export function getServiceInfoByName(commandName: string): ServiceInfo {
  const projectDir = findProjectDir();

  // First check if there's a running process with this name
  const runningProcesses = findProcessesByCommandNameAndProjectDir(commandName, projectDir);
  if (runningProcesses.length > 0) {
    // Process is running - return DB info
    const runningProcess = runningProcesses[0];

    // Also try to get config info if available
    let serviceConfig: ServiceConfig | undefined;
    try {
      const configResult = getServiceConfigByName(commandName);
      serviceConfig = configResult?.serviceConfig;
    } catch {
      // Service not in config, that's fine for transient processes
    }

    return {
      commandName: runningProcess.command_name,
      projectDir: runningProcess.project_dir,
      runningProcess,
      serviceConfig,
    };
  }

  // No running process - try to find in config
  const configResult = getServiceConfigByName(commandName);

  return {
    commandName: configResult.serviceConfig.name,
    projectDir: configResult.projectDir,
    serviceConfig: configResult.serviceConfig,
  };
}
