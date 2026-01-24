import { describe, test, expect, afterAll, beforeAll, afterEach } from 'vitest';
import { TestWorkspace } from '../TestWorkspace.ts';

const workspace = new TestWorkspace('cli-port-reservation');

afterAll(() => workspace.cleanup());

describe('CLI Port Reservation Commands', () => {
  // Clean up any existing port reservations before tests
  beforeAll(async () => {
    await workspace.runCli(['release-ports'], { ignoreExitCode: true });
  });

  describe('reserve-port command', () => {
    test('should reserve a port for a service', async () => {
      const result = await workspace.runCli(['reserve-port', 'echo']);
      expect(result.stdoutAsString()).toMatch(/Reserved port \d+ for service 'echo'/);
    });

    test('should error when reserving port for service that already has one', async () => {
      const result = await workspace.runCli(['reserve-port', 'echo'], { ignoreExitCode: true });
      expect(result.stderrAsString()).toContain("already has a reserved port");
    });

    test('should reserve a project-level port', async () => {
      const result = await workspace.runCli(['reserve-port']);
      expect(result.stdoutAsString()).toMatch(/Reserved port \d+ for project/);
    });
  });

  describe('get-reserved-port command', () => {
    test('should return the reserved port for a service', async () => {
      const result = await workspace.runCli(['get-reserved-port', 'echo']);
      const port = parseInt(result.stdoutAsString().trim());
      expect(port).toBeGreaterThanOrEqual(4000);
      expect(port).toBeLessThanOrEqual(65535);
    });

    test('should error when no port reserved for service', async () => {
      const result = await workspace.runCli(['get-reserved-port', 'nonexistent'], { ignoreExitCode: true });
      expect(result.stderrAsString()).toContain("No reserved port found");
    });

    test('should require service name', async () => {
      const result = await workspace.runCli(['get-reserved-port'], { ignoreExitCode: true });
      expect(result.stderrAsString()).toContain("Not enough non-option arguments");
    });
  });

  describe('list-reserved-ports command', () => {
    test('should list reserved ports for project', async () => {
      const result = await workspace.runCli(['list-reserved-ports']);
      expect(result.stdoutAsString()).toContain('Reserved ports:');
      expect(result.stdoutAsString()).toContain('echo');
    });

    test('should show port details', async () => {
      const result = await workspace.runCli(['list-reserved-ports']);
      expect(result.stdoutAsString()).toMatch(/Port \d+/);
      expect(result.stdoutAsString()).toContain('Reserved at:');
    });
  });

  describe('release-ports command', () => {
    test('should release port for a specific service', async () => {
      // First reserve a port for server
      await workspace.runCli(['reserve-port', 'server']);

      // Then release it
      const result = await workspace.runCli(['release-ports', 'server']);
      expect(result.stdoutAsString()).toMatch(/Released port \d+ for service 'server'/);

      // Verify it's gone
      const getResult = await workspace.runCli(['get-reserved-port', 'server'], { ignoreExitCode: true });
      expect(getResult.stderrAsString()).toContain("No reserved port found");
    });

    test('should error when no port to release for service', async () => {
      const result = await workspace.runCli(['release-ports', 'nonexistent'], { ignoreExitCode: true });
      expect(result.stderrAsString()).toContain("No reserved port found");
    });

    test('should release all ports for project', async () => {
      // Reserve a port for server again
      await workspace.runCli(['reserve-port', 'server']);

      const result = await workspace.runCli(['release-ports']);
      expect(result.stdoutAsString()).toMatch(/Released \d+ reserved port/);

      // Verify all are gone
      const listResult = await workspace.runCli(['list-reserved-ports']);
      expect(listResult.stdoutAsString()).toContain('No reserved ports found');
    });
  });

  describe('PORT env var integration', () => {
    test('should set PORT env var when starting service with reserved port', async () => {
      // Reserve a port for echo
      const reserveResult = await workspace.runCli(['reserve-port', 'echo']);
      const portMatch = reserveResult.stdoutAsString().match(/Reserved port (\d+)/);
      const port = portMatch?.[1];
      expect(port).toBeDefined();

      // Start the service
      await workspace.runCli(['start', 'echo']);

      // Check the logs to see if PORT was set
      // Wait a bit for logs to be captured
      await new Promise(resolve => setTimeout(resolve, 500));

      const logsResult = await workspace.runCli(['logs', 'echo']);
      expect(logsResult.stdoutAsString()).toContain(`PORT is ${port}`);

      // Cleanup
      await workspace.runCli(['kill', 'echo'], { ignoreExitCode: true });
      await workspace.runCli(['release-ports', 'echo']);
    });

    test('should not set PORT when service has no reserved port', async () => {
      // Make sure server has no reserved port
      await workspace.runCli(['release-ports', 'server'], { ignoreExitCode: true });

      // Start the service
      await workspace.runCli(['start', 'server']);

      // Wait a bit for logs to be captured
      await new Promise(resolve => setTimeout(resolve, 500));

      const logsResult = await workspace.runCli(['logs', 'server']);
      // When PORT is not set, echo will output empty string for $PORT
      expect(logsResult.stdoutAsString()).toContain('Server running on port');
      expect(logsResult.stdoutAsString()).not.toMatch(/Server running on port \d+/);

      // Cleanup
      await workspace.runCli(['kill', 'server'], { ignoreExitCode: true });
    });
  });
});

describe('MCP Port Reservation Tools', () => {
  let app: ReturnType<typeof workspace.createMcpApp>;

  beforeAll(async () => {
    await workspace.runCli(['release-ports'], { ignoreExitCode: true });
  });

  afterEach(async () => {
    if (app) {
      await app.close();
    }
  });

  test('should reserve port via MCP', async () => {
    app = workspace.createMcpApp();

    const result = await app.callTool('ReservePort', { serviceName: 'echo' });
    expect(result.isError).toBe(false);
    const content = JSON.parse(result.getTextContent() ?? '{}');
    expect(content.port).toBeGreaterThanOrEqual(4000);
    expect(content.serviceName).toBe('echo');
  });

  test('should get reserved port via MCP', async () => {
    app = workspace.createMcpApp();

    const result = await app.callTool('GetReservedPort', { serviceName: 'echo' });
    expect(result.isError).toBe(false);
    const content = JSON.parse(result.getTextContent() ?? '{}');
    expect(content.port).toBeGreaterThanOrEqual(4000);
  });

  test('should list reserved ports via MCP', async () => {
    app = workspace.createMcpApp();

    const result = await app.callTool('ListReservedPorts', {});
    expect(result.isError).toBe(false);
    const content = JSON.parse(result.getTextContent() ?? '{}');
    expect(content.ports).toBeInstanceOf(Array);
    expect(content.ports.length).toBeGreaterThan(0);
  });

  test('should release ports via MCP', async () => {
    app = workspace.createMcpApp();

    const result = await app.callTool('ReleasePorts', { serviceName: 'echo' });
    expect(result.isError).toBe(false);
    const content = JSON.parse(result.getTextContent() ?? '{}');
    expect(content.releasedCount).toBe(1);
  });
});
