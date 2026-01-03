// Public API for @facetlayer/candle
// This file exports functions and types for programmatic usage (e.g., GUI apps)

// Process listing
export { handleList, printListOutput } from './list-command.ts';
export type { ListOutput } from './list-command.ts';

// Process logs
export { getProcessLogs } from './logs/processLogs.ts';
export type { ProcessLog } from './logs/processLogs.ts';
export { LatestExecutionLogFilter as AfterProcessStartLogFilter } from './log-filters/LatestExecutionLogFilter.ts';

// Configuration
export { findConfigFile, getServiceConfigByName } from './configFile.ts';
export type { CandleSetupConfig, ServiceConfig } from './configFile.ts';

// Database access
export { getDatabase } from './database/database.ts';
export {
  findAllProcesses,
  findProcessesByProjectDir,
  findProcessesByCommandNameAndProjectDir
} from './database/processTable.ts';
export type { ProcessEntry } from './database/processTable.ts';
