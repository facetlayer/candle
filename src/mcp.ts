import { Server } from '@modelcontextprotocol/sdk/server/index.js';
import { StdioServerTransport } from '@modelcontextprotocol/sdk/server/stdio.js';
import {
  CallToolRequestSchema,
  ErrorCode,
  ListToolsRequestSchema,
  McpError,
} from '@modelcontextprotocol/sdk/types.js';
import { addServerConfig } from './addServerConfig.ts';
import { handleKill } from './handleKill.ts';
import { handleList } from './handleList.ts';
import { handleLogs } from './handleLogs.ts';
import { handleRestart } from './handleRestart.ts';
import { handleRun } from './handleRun.ts';
import { infoLog } from './logs.ts';

// Console.log wrapper for collecting logs
class ConsoleLogWrapper {
  private collectedLogs: string[] = [];
  private originalConsoleLog = console.log;
  private isWrapped = false;

  wrap() {
    if (this.isWrapped) return;

    console.log = (...args: any[]) => {
      const logMessage = args
        .map(arg => (typeof arg === 'string' ? arg : JSON.stringify(arg)))
        .join(' ');
      this.collectedLogs.push(logMessage);
      this.originalConsoleLog(...args);
    };
    this.isWrapped = true;
  }

  reset() {
    console.log = this.originalConsoleLog;
    this.isWrapped = false;
  }

  takeLogs(): string[] {
    const logs = [...this.collectedLogs];
    this.collectedLogs = [];
    return logs;
  }
}

async function callWrapped(handler: (args: any) => Promise<any>, args: any) {
  const logWrapper = new ConsoleLogWrapper();
  logWrapper.wrap();

  let result: any = undefined;
  let error: any = undefined;

  try {
    result = await handler(args);
  } catch (e) {
    error = e;
  } finally {
    logWrapper.reset();
  }

  if (error) {
    error = {
      message: error?.message,
      stack: error?.stack,
      ...error,
    };
  }

  return {
    result,
    error,
    logs: logWrapper.takeLogs(),
  };
}

const DEFAULT_LOGS_LIMIT = 200;

export interface ToolDefinition {
  name: string;
  description: string;
  inputSchema: {
    type: 'object';
    properties: Record<string, any>;
    required?: string[];
  };
  handler: (args: any) => Promise<any>;
}

const toolDefinitions: ToolDefinition[] = [
  {
    name: 'ListServices',
    description: 'List services with structured output',
    inputSchema: {
      type: 'object',
      properties: {
        showAll: {
          type: 'boolean',
          description: 'Show all services or just current directory (optional)',
        },
      },
    },
    handler: async args => {
      const showAll = args?.showAll as boolean | undefined;
      const listOutput = await handleList({ showAll });
      return listOutput;
    },
  },
  {
    name: 'GetLogs',
    description: 'Get recent logs for a specific service',
    inputSchema: {
      type: 'object',
      properties: {
        name: {
          type: 'string',
          description: 'Name of the service to get logs for',
        },
        limit: {
          type: 'number',
          description: 'Maximum number of log lines to return (optional)',
        },
      },
      required: ['name'],
    },
    handler: async args => {
      const result = await handleLogs({
        commandName: args?.name as string,
        limit: args?.limit ?? DEFAULT_LOGS_LIMIT,
      });
      return result;
    },
  },
  {
    name: 'StartService',
    description: 'Start a specific service',
    inputSchema: {
      type: 'object',
      properties: {
        name: {
          type: 'string',
          description:
            'Name of the service to start (optional - uses default service if not provided)',
        },
      },
    },
    handler: async args => {
      const result = await handleRun({
        commandName: args?.name as string,
        watchLogs: false,
        consoleOutputFormat: 'pretty',
      });
      return result;
    },
  },
  {
    name: 'KillService',
    description: 'Kill a running service',
    inputSchema: {
      type: 'object',
      properties: {
        name: {
          type: 'string',
          description:
            'Name of the service to kill (optional - uses default service if not provided)',
        },
      },
    },
    handler: async args => {
      await handleKill({
        commandName: args?.name as string,
      });
    },
  },
  {
    name: 'RestartService',
    description: 'Restart a running service',
    inputSchema: {
      type: 'object',
      properties: {
        name: {
          type: 'string',
          description:
            'Name of the service to restart (optional - uses default service if not provided)',
        },
      },
    },
    handler: async args => {
      const result = await handleRestart({
        commandName: args?.name as string,
        consoleOutputFormat: 'pretty',
        watchLogs: false,
      });
      return result;
    },
  },
  {
    name: 'AddServerConfig',
    description: 'Add a new server configuration to .candle-setup.json',
    inputSchema: {
      type: 'object',
      properties: {
        name: {
          type: 'string',
          description: 'Name of the service',
        },
        shell: {
          type: 'string',
          description: 'Shell command to run the service',
        },
        root: {
          type: 'string',
          description: 'Root directory for the service (optional)',
        },
      },
      required: ['name', 'shell'],
    },
    handler: async args => {
      const { name, shell, root } = args;

      if (!name || !shell) {
        throw new McpError(ErrorCode.InvalidRequest, 'Service name and shell command are required');
      }

      addServerConfig({
        name,
        shell,
        root,
      });

      console.log(`Service '${name}' added successfully to .candle-setup.json`);
    },
  },
];

export async function serveMCP() {
  infoLog('MCP: Starting MCP server');

  const packageInfo = await import('../package.json');

  // Create server with proper initialization
  const server = new Server(
    {
      name: packageInfo.name,
      version: packageInfo.version,
    },
    {
      capabilities: {
        tools: {},
      },
      instructions:
        'Tool for running and managing local dev servers. Use this when launching any local servers, including ' +
        'web servers, APIs, and other services.',
    }
  );

  // Register tool list handler
  server.setRequestHandler(ListToolsRequestSchema, async request => {
    infoLog('MCP: Received ListTools request:', request);
    const response = {
      tools: toolDefinitions.map(tool => ({
        name: tool.name,
        description: tool.description,
        inputSchema: tool.inputSchema,
      })),
    };

    infoLog('MCP: Responding to ListTools:', response);
    return response;
  });

  // Register tool call handler
  server.setRequestHandler(CallToolRequestSchema, async request => {
    const { name, arguments: args } = request.params;
    infoLog('MCP: Received CallTool request:', request);

    const tool = toolDefinitions.find(t => t.name === name);
    if (!tool) {
      infoLog(`MCP: CallTool error - Unknown tool: ${name}`);
      throw new McpError(ErrorCode.MethodNotFound, `Unknown tool: ${name}`);
    }

    const callResult = await callWrapped(tool.handler, args);
    return callResult;
  });

  // Create transport and connect
  const transport = new StdioServerTransport();
  await server.connect(transport);
  infoLog('MCP: Server launched and connected');
}

export async function main(): Promise<void> {
  await serveMCP();
}
