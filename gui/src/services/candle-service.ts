import { createEndpoint } from '@facetlayer/prism-framework-api';
import type { ServiceDefinition } from '@facetlayer/prism-framework-api';
import { z } from 'zod';
import {
  findAllProcesses,
  getProcessLogs,
} from '@facetlayer/candle';
import { formatUptime } from '../../../src/list-command.ts';
import { handleStartCommand } from '../../../src/start-command.ts';
import { handleKillCommand } from '../../../src/kill-command.ts';
import { handleRestart } from '../../../src/restart-command.ts';
import { handleListPorts } from '../../../src/list-ports-command.ts';

const listServices = createEndpoint({
  method: 'GET',
  path: '/services',
  description: 'List all running services across the system',
  handler: async () => {
    const processEntries = findAllProcesses();
    const processes = processEntries.map(entry => ({
      serviceName: entry.command_name,
      projectDir: entry.project_dir,
      pid: entry.pid,
      uptime: formatUptime(Date.now() - entry.start_time * 1000),
      status: 'RUNNING',
      shell: entry.shell,
      root: entry.root || null,
    }));
    return { processes };
  },
});

const getServiceLogs = createEndpoint({
  method: 'GET',
  path: '/services/:name/logs',
  description: 'Get recent logs for a service',
  requestSchema: z.object({
    name: z.string(),
    projectDir: z.string(),
    afterLogId: z.coerce.number().optional(),
    limit: z.coerce.number().optional(),
  }),
  handler: async (input) => {
    const logs = getProcessLogs({
      projectDir: input.projectDir,
      commandNames: [input.name],
      limit: input.limit || 200,
      afterLogId: input.afterLogId,
    });
    return { logs };
  },
});

const startService = createEndpoint({
  method: 'POST',
  path: '/services/:name/start',
  description: 'Start a service',
  requestSchema: z.object({
    name: z.string(),
    projectDir: z.string(),
  }),
  handler: async (input) => {
    // chdir to the project so handleStartCommand can find the .candle.json
    const originalCwd = process.cwd();
    try {
      process.chdir(input.projectDir);
      await handleStartCommand({
        projectDir: input.projectDir,
        commandNames: [input.name],
        consoleOutputFormat: 'json',
      });
    } finally {
      process.chdir(originalCwd);
    }
    return { success: true, message: `Service '${input.name}' started` };
  },
});

const restartService = createEndpoint({
  method: 'POST',
  path: '/services/:name/restart',
  description: 'Restart a service',
  requestSchema: z.object({
    name: z.string(),
    projectDir: z.string(),
  }),
  handler: async (input) => {
    await handleRestart({
      projectDir: input.projectDir,
      commandNames: [input.name],
      consoleOutputFormat: 'json',
    });
    return { success: true, message: `Service '${input.name}' restarted` };
  },
});

const killService = createEndpoint({
  method: 'POST',
  path: '/services/:name/kill',
  description: 'Kill a running service',
  requestSchema: z.object({
    name: z.string(),
    projectDir: z.string(),
  }),
  handler: async (input) => {
    await handleKillCommand({
      projectDir: input.projectDir,
      commandNames: [input.name],
    });
    return { success: true, message: `Service '${input.name}' killed` };
  },
});

const getServiceUrl = createEndpoint({
  method: 'GET',
  path: '/services/:name/url',
  description: 'Get the localhost URL for a running service',
  requestSchema: z.object({
    name: z.string(),
    projectDir: z.string(),
  }),
  handler: async (input) => {
    // handleListPorts uses findConfigFile(cwd) internally
    const originalCwd = process.cwd();
    try {
      process.chdir(input.projectDir);
      const result = await handleListPorts({ serviceName: input.name });
      if (result.ports.length === 0) {
        return { url: null };
      }
      const sorted = [...result.ports].sort((a, b) => a.port - b.port);
      return { url: `http://localhost:${sorted[0].port}` };
    } finally {
      process.chdir(originalCwd);
    }
  },
});

export const candleService: ServiceDefinition = {
  name: 'candle',
  endpoints: [
    listServices,
    getServiceLogs,
    startService,
    restartService,
    killService,
    getServiceUrl,
  ],
};
