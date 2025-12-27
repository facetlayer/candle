import { describe, it, expect, beforeAll, afterEach } from 'vitest';
import * as fs from 'fs';
import * as path from 'path';
import { createRunCandleCommand, getTestTempDirectory, getSampleServersDirectory } from './utils';

const TEST_STATE_DIR = getTestTempDirectory('transient-db');
const SAMPLE_SERVERS_DIR = getSampleServersDirectory();
// Run commands from sampleServers directory where .candle-setup.json exists
const runCandleCommand = createRunCandleCommand(TEST_STATE_DIR, SAMPLE_SERVERS_DIR);

describe('Transient Processes', () => {
  beforeAll(() => {
    if (fs.existsSync(TEST_STATE_DIR)) {
      fs.rmSync(TEST_STATE_DIR, { recursive: true, force: true });
    }
    fs.mkdirSync(TEST_STATE_DIR, { recursive: true });
  });

  afterEach(async () => {
    await runCandleCommand(['kill-all']).catch(() => {});
  });

  describe('Basic Operations', () => {
    it('should start a transient process with --shell', async () => {
      const result = await runCandleCommand([
        'start',
        'my-transient',
        '--shell',
        'node testProcess.js',
      ]);

      expect(result.code).toBe(0);
      expect(result.stdout).toContain('Started');
      expect(result.stdout).toContain('my-transient');

      // Verify it appears in list
      const listResult = await runCandleCommand(['list']);
      expect(listResult.stdout).toContain('my-transient');
      expect(listResult.stdout).toContain('RUNNING');
    });

    it('should start a transient process with --shell and --root', async () => {
      // Use 'test' subdirectory which exists in sampleServers
      const result = await runCandleCommand([
        'start',
        'rooted-transient',
        '--shell',
        'node ../testProcess.js',
        '--root',
        'test',
      ]);

      expect(result.code).toBe(0);
      expect(result.stdout).toContain('Started');
    });

    it('should show logs for a transient process', async () => {
      const startResult = await runCandleCommand([
        'start',
        'log-transient',
        '--shell',
        'node echoServer.js',
      ]);
      expect(startResult.code).toBe(0);

      const waitResult = await runCandleCommand(['wait-for-log', 'log-transient', '--message', 'Echo server started']);
      expect(waitResult.code).toBe(0);

      const logsResult = await runCandleCommand(['logs', 'log-transient']);
      expect(logsResult.stdout).toContain('Echo server started');
    });

    it('should kill a transient process', async () => {
      const startResult = await runCandleCommand([
        'start',
        'kill-me',
        '--shell',
        'node testProcess.js',
      ]);
      expect(startResult.code).toBe(0);

      const waitResult = await runCandleCommand(['wait-for-log', 'kill-me', '--message', 'Test server started']);
      expect(waitResult.code).toBe(0);

      const killResult = await runCandleCommand(['kill', 'kill-me']);
      expect(killResult.code).toBe(0);
      expect(killResult.stdout).toContain('Killed');

      // Verify it's gone from list
      const listResult = await runCandleCommand(['list']);
      expect(listResult.stdout).not.toContain('kill-me');
    });

    it('should restart a transient process with same shell from DB', async () => {
      const startResult = await runCandleCommand([
        'start',
        'restart-me',
        '--shell',
        'node testProcess.js',
      ]);
      expect(startResult.code).toBe(0);

      const waitResult = await runCandleCommand(['wait-for-log', 'restart-me', '--message', 'Test server started']);
      expect(waitResult.code).toBe(0);

      const restartResult = await runCandleCommand(['restart', 'restart-me']);
      expect(restartResult.code).toBe(0);
      expect(restartResult.stdout).toContain('Started');

      // Verify it's still running
      const listResult = await runCandleCommand(['list']);
      expect(listResult.stdout).toContain('restart-me');
      expect(listResult.stdout).toContain('RUNNING');
    });
  });

  describe('Validation', () => {
    it('should error when --root is provided without --shell', async () => {
      const result = await runCandleCommand(['start', 'bad-transient', '--root', 'src']);

      // Without --shell, it will try to find 'bad-transient' in config
      expect(result.code).not.toBe(0);
      expect(result.stderr).toContain('bad-transient');
    });

    it('should error when starting transient in directory without config file', async () => {
      const result = await runCandleCommand(
        ['start', 'no-config', '--shell', 'echo hello'],
        { cwd: '/tmp' }
      );

      expect(result.code).not.toBe(0);
      expect(result.stderr).toContain('.candle.json');
    });

    it('should error when --root escapes project directory', async () => {
      const result = await runCandleCommand([
        'start',
        'escape-transient',
        '--shell',
        'echo hello',
        '--root',
        '../../../escape',
      ]);

      expect(result.code).not.toBe(0);
      expect(result.stderr).toContain('root');
    });
  });

  describe('Name Collision Behavior', () => {
    it('should allow transient with same name as config service', async () => {
      // 'web' is defined in .candle-setup.json
      // Start it as transient with different shell
      const result = await runCandleCommand([
        'start',
        'web',
        '--shell',
        'node echoServer.js',
      ]);

      expect(result.code).toBe(0);
      expect(result.stdout).toContain('Started');

      // Verify it's running with our shell (check logs for echoServer output)
      await runCandleCommand(['wait-for-log', 'web', '--message', 'Echo server started']);
      const logsResult = await runCandleCommand(['logs', 'web']);
      expect(logsResult.stdout).toContain('Echo server started');
    });

    it('should kill existing transient when starting new one with same name', async () => {
      // Start first transient
      const startResult1 = await runCandleCommand([
        'start',
        'same-name',
        '--shell',
        'node testProcess.js',
      ]);
      expect(startResult1.code).toBe(0);

      const waitResult1 = await runCandleCommand(['wait-for-log', 'same-name', '--message', 'Test server started']);
      expect(waitResult1.code).toBe(0);

      // Start second transient with same name but different shell
      const result = await runCandleCommand([
        'start',
        'same-name',
        '--shell',
        'node echoServer.js',
      ]);

      expect(result.code).toBe(0);

      // Verify new one is running
      const waitResult2 = await runCandleCommand(['wait-for-log', 'same-name', '--message', 'Echo server started']);
      expect(waitResult2.code).toBe(0);

      const logsResult = await runCandleCommand(['logs', 'same-name']);
      expect(logsResult.stdout).toContain('Echo server started');
    });

    it('should not affect other transient when starting config service', async () => {
      // Start a transient
      await runCandleCommand([
        'start',
        'my-transient',
        '--shell',
        'node testProcess.js',
      ]);
      await runCandleCommand(['wait-for-log', 'my-transient', '--message', 'Test server started']);

      // Start a config service
      await runCandleCommand(['start', 'echo']);
      await runCandleCommand(['wait-for-log', 'echo', '--message', 'Echo server started']);

      // Both should be running
      const listResult = await runCandleCommand(['list']);
      expect(listResult.stdout).toContain('my-transient');
      expect(listResult.stdout).toContain('echo');
    });
  });

  describe('Config Drift Detection', () => {
    it('should not show warning when config matches DB', async () => {
      await runCandleCommand(['start', 'echo']);
      await runCandleCommand(['wait-for-log', 'echo', '--message', 'Echo server started']);

      const listResult = await runCandleCommand(['list']);
      expect(listResult.stdout).toContain('echo');
      expect(listResult.stdout).toContain('RUNNING');
      expect(listResult.stdout).not.toContain('[config changed]');
    });

    it('should show warning when transient shadows config service', async () => {
      // Start 'echo' as transient with different shell
      await runCandleCommand([
        'start',
        'echo',
        '--shell',
        'node testProcess.js',
      ]);
      await runCandleCommand(['wait-for-log', 'echo', '--message', 'Test server started']);

      const listResult = await runCandleCommand(['list']);
      expect(listResult.stdout).toContain('echo');
      expect(listResult.stdout).toContain('[config changed]');
    });

    it('should not show warning for pure transient (not in config)', async () => {
      await runCandleCommand([
        'start',
        'pure-transient',
        '--shell',
        'node testProcess.js',
      ]);
      await runCandleCommand(['wait-for-log', 'pure-transient', '--message', 'Test server started']);

      const listResult = await runCandleCommand(['list']);
      expect(listResult.stdout).toContain('pure-transient');
      expect(listResult.stdout).not.toContain('[config changed]');
    });
  });

  describe('Database Schema', () => {
    it('should store shell in DB for config-based process', async () => {
      await runCandleCommand(['start', 'echo']);
      await runCandleCommand(['wait-for-log', 'echo', '--message', 'Echo server started']);

      // Restart should work even though we're reading from DB
      const restartResult = await runCandleCommand(['restart', 'echo']);
      expect(restartResult.code).toBe(0);
      expect(restartResult.stdout).toContain('Started');
    });

    it('should store shell in DB for transient process', async () => {
      await runCandleCommand([
        'start',
        'db-transient',
        '--shell',
        'node testProcess.js',
      ]);
      await runCandleCommand(['wait-for-log', 'db-transient', '--message', 'Test server started']);

      // Restart reads from DB
      const restartResult = await runCandleCommand(['restart', 'db-transient']);
      expect(restartResult.code).toBe(0);

      // Verify it still works
      await runCandleCommand(['wait-for-log', 'db-transient', '--message', 'Test server started']);
    });
  });

  describe('Edge Cases', () => {
    it('should handle shell command with spaces', async () => {
      const result = await runCandleCommand([
        'start',
        'spaced-shell',
        '--shell',
        'node testProcess.js',
      ]);

      expect(result.code).toBe(0);
      expect(result.stdout).toContain('Started');
    });

    it('should handle process that exits immediately', async () => {
      const result = await runCandleCommand([
        'start',
        'quick-exit',
        '--shell',
        'echo "quick exit"',
      ]);

      // Process may have already exited, but start should succeed
      expect(result.code).toBe(0);
    });

    it('should handle name with dash', async () => {
      const result = await runCandleCommand([
        'start',
        'my-dashed-name',
        '--shell',
        'node testProcess.js',
      ]);

      expect(result.code).toBe(0);
      expect(result.stdout).toContain('my-dashed-name');
    });
  });
});
