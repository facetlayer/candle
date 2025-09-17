import * as fs from 'fs';
import * as path from 'path';
import { MissingServiceWithNameError, MissingSetupFileError } from './errors.ts';

export interface ServiceConfig {
    name: string;
    shell: string;
    root?: string;
}

export interface CandleSetupConfig {
    services?: ServiceConfig[];
}

/*
  findProjectDir

  Finds the location of the nearest .candle-setup.json file.
*/
export function findProjectDir(cwd: string = process.cwd()): string {
    const setupResult = findConfigFile(cwd);
    if (setupResult) {
        return path.dirname(setupResult.projectDir);
    }

    throw new MissingSetupFileError(cwd);
}

export function findConfigFile(currentDir: string): { config: CandleSetupConfig, projectDir: string } | null {
    currentDir = currentDir ? path.resolve(currentDir) : process.cwd();
    
    while (true) {
        const configFilePath = path.join(currentDir, '.candle-setup.json');
        
        if (fs.existsSync(configFilePath)) {
            try {
                const content = fs.readFileSync(configFilePath, 'utf8');
                const config = JSON.parse(content) as CandleSetupConfig;
                validateConfig(config);
                return { config, projectDir: currentDir };
            } catch (error) {
                throw new Error(`Invalid .candle-setup.json at ${configFilePath}: ${error.message}`);
            }
        }
        
        const parentDir = path.dirname(currentDir);
        if (parentDir === currentDir) {
            // Reached root directory
            break;
        }
        currentDir = parentDir;
    }
    
    return null;
}

export function validateConfig(config: CandleSetupConfig): void {
    const names = new Set<string>();
    
    for (const service of config.services || []) {
        if (!service.name || typeof service.name !== 'string') {
            throw new Error('Config file error: Each service must have a "name" string');
        }
        
        if (!service.shell || typeof service.shell !== 'string') {
            throw new Error(`Config file error: Service "${service.name}" must have a "shell" string`);
        }
        
        if (names.has(service.name)) {
            throw new Error(`Config file error: Duplicate service name: "${service.name}"`);
        }
        names.add(service.name);
        
        if (service.root && !isValidRelativePath(service.root)) {
            throw new Error(`Service "${service.name}" has invalid root path: "${service.root}"`);
        }
    }
}

function isValidRelativePath(p: string): boolean {
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

export function getServiceCwd(service: ServiceConfig, configPath: string): string {
    const configDir = path.dirname(configPath);
    if (service.root) {
        return path.resolve(configDir, service.root);
    }
    return configDir;
}

export function findServiceByName(config: CandleSetupConfig, name?: string): ServiceConfig | null {
    if (!name) {
        // No name provided - use default service
        const defaultService = config.services[0];
        if (defaultService) {
            return defaultService;
        }
        
        // If no default is marked, return null to indicate we need a service name
        return null;
    }
    
    return config.services.find(s => s.name === name) || null;
}

export function getAllServiceNames(config: CandleSetupConfig): string[] {
    return config.services.map(s => s.name);
}

interface FoundServiceConfig {
    serviceConfig: ServiceConfig
    projectDir: string
}

function findLooseCommandName(commandName: string, config: CandleSetupConfig, projectDir: string, currentDir?: string): ServiceConfig | null {
    const searchDir = currentDir || process.cwd();
    
    // Find all services where commandName is a substring of the service name
    const matchingServices = config.services.filter(service => 
        service.name.includes(commandName)
    );
    
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
        throw new Error(`Ambiguous service name "${commandName}". Multiple services match in current directory: ${serviceNames}`);
    }
    
    // No services match current directory, try parent directory
    const parentDir = path.dirname(searchDir);
    if (parentDir === searchDir || parentDir === projectDir) {
        // Reached root or project directory, return null if no exact match
        return matchingServices.length === 1 ? matchingServices[0] : null;
    }
    
    return findLooseCommandName(commandName, config, projectDir, parentDir);
}


export function getServiceConfigByName(commandName?: string, currentDir?: string): FoundServiceConfig | null {
    const foundConfig = findConfigFile(currentDir);
    if (!foundConfig) {
        throw new MissingSetupFileError(currentDir);
    }

    const { config, projectDir } = foundConfig;

    let serviceConfig: ServiceConfig | null = null;

    if (!commandName) {
        serviceConfig = config.services[0];
    } else {
        serviceConfig = config.services.find(s => s.name === commandName) || null;
        
        if (!serviceConfig) {
            // Try loose matching if exact match fails
            serviceConfig = findLooseCommandName(commandName, config, projectDir, currentDir);
        }
    }

    if (!serviceConfig) { 
        throw new MissingServiceWithNameError(projectDir, commandName);
    }

    return {
        serviceConfig,
        projectDir
    };
}
