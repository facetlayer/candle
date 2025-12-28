import * as path from 'path';
import { mcpShell, MCPStdinSubprocess } from 'expect-mcp';
import { getCliPath } from '../utils';

const CLI_PATH = getCliPath();
const DEFAULT_TEST_STATE_DIR = path.join(__dirname, 'db');

export interface CreateMcpAppOptions {
  cwd?: string;
  allowDebugLogging?: boolean;
  databaseDir?: string;
}

export function createMcpApp(options: CreateMcpAppOptions = {}): MCPStdinSubprocess {
  const {
    cwd = __dirname,
    allowDebugLogging = false,
    databaseDir = DEFAULT_TEST_STATE_DIR,
  } = options;

  return mcpShell(`node ${CLI_PATH} --mcp`, {
    allowDebugLogging,
    cwd,
    env: {
      ...process.env,
      CANDLE_DATABASE_DIR: databaseDir,
    },
  });
}
