import * as fs from 'fs';
import * as path from 'path';
import { findConfigFile, validateConfig } from './configFile.ts';
import type { ServiceConfig, CandleSetupConfig } from './configFile.ts';

export interface AddServerConfigArgs {
    name: string;
    shell: string;
    root?: string;
    env?: Record<string, string>;
}

export function addServerConfig(args: AddServerConfigArgs, startDir: string = process.cwd()): void {
    const configPath = findOrCreateSetupFile(startDir);
    
    // Read existing config
    const content = fs.readFileSync(configPath, 'utf8');
    const config = JSON.parse(content) as CandleSetupConfig;
    
    // Check if service name already exists
    if (config.services.some(service => service.name === args.name)) {
        throw new Error(`Service '${args.name}' already exists in configuration`);
    }
    
    // Create new service config
    const newService: ServiceConfig = {
        name: args.name,
        shell: args.shell,
        ...(args.root && { root: args.root }),
        ...(args.env && { env: args.env })
    };
    
    // Add new service to config
    config.services.push(newService);
    
    // Validate the updated config
    validateConfig(config);
    
    // Write back to file
    fs.writeFileSync(configPath, JSON.stringify(config, null, 2));
}

function findOrCreateSetupFile(startDir: string): string {
    const setupResult = findConfigFile(startDir);
    
    if (setupResult) {
        return setupResult.projectDir;
    }
    
    // Create new .candle-setup.json file in the current directory
    const configPath = path.join(startDir, '.candle-setup.json');
    const initialConfig: CandleSetupConfig = {
        services: []
    };
    
    fs.writeFileSync(configPath, JSON.stringify(initialConfig, null, 2));
    return configPath;
}
