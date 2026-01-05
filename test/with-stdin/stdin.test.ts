import { describe, it, expect, afterAll, beforeAll } from 'vitest';
import { TestWorkspace } from '../TestWorkspace';
import { createStdinMessage } from '../../src/database/stdinMessagesTable';

const workspace = new TestWorkspace('with-stdin');

let originalDbDir: string | undefined;

// Set the database directory to match the workspace so stdin messages go to the right place
beforeAll(() => {
  originalDbDir = process.env.CANDLE_DATABASE_DIR;
  process.env.CANDLE_DATABASE_DIR = workspace.dbDir;
});

afterAll(async () => {
  await workspace.cleanup();
  // Restore original database directory
  if (originalDbDir !== undefined) {
    process.env.CANDLE_DATABASE_DIR = originalDbDir;
  } else {
    delete process.env.CANDLE_DATABASE_DIR;
  }
});

describe('Stdin Feature', () => {
  describe('config-based service with enableStdin', () => {
    it('should start a service with enableStdin from config', async () => {
      const result = await workspace.runCli(['start', 'stdin-echo']);

      expect(result.stdoutAsString()).toContain('Started');
      expect(result.stdoutAsString()).toContain('stdin-echo');

      // Verify it appears in list
      const listResult = await workspace.runCli(['list']);
      expect(listResult.stdoutAsString()).toContain('stdin-echo');
      expect(listResult.stdoutAsString()).toContain('RUNNING');
    });

    it('should receive stdin messages sent via database', async () => {
      // Start the service
      await workspace.runCli(['start', 'stdin-echo']);
      await workspace.runCli(['wait-for-log', 'stdin-echo', '--message', 'Stdin echo server started']);

      // Send a message via the database
      createStdinMessage({
        commandName: 'stdin-echo',
        projectDir: workspace.dbDir,
        data: 'Hello from database!\n',
      });

      // Wait for the message to be received and echoed
      await workspace.runCli(['wait-for-log', 'stdin-echo', '--message', '[RECEIVED] Hello from database!']);

      // Verify the message appears in logs
      const logsResult = await workspace.runCli(['logs', 'stdin-echo']);
      expect(logsResult.stdoutAsString()).toContain('[RECEIVED] Hello from database!');
    });

    it('should handle multiple stdin messages', async () => {
      // Start a fresh service
      await workspace.runCli(['start', 'stdin-echo']);
      await workspace.runCli(['wait-for-log', 'stdin-echo', '--message', 'Stdin echo server started']);

      // Send multiple messages
      createStdinMessage({
        commandName: 'stdin-echo',
        projectDir: workspace.dbDir,
        data: 'Message 1\n',
      });

      createStdinMessage({
        commandName: 'stdin-echo',
        projectDir: workspace.dbDir,
        data: 'Message 2\n',
      });

      // Wait for the messages to be processed (give it time for polling)
      await new Promise(resolve => setTimeout(resolve, 1500));

      // Verify messages appear in logs
      const logsResult = await workspace.runCli(['logs', 'stdin-echo']);
      expect(logsResult.stdoutAsString()).toContain('[RECEIVED] Message 1');
      expect(logsResult.stdoutAsString()).toContain('[RECEIVED] Message 2');
    });
  });

  describe('transient process with --enable-stdin', () => {
    it('should start transient process with --enable-stdin flag', async () => {
      const result = await workspace.runCli([
        'start',
        'transient-stdin',
        '--shell',
        'node ../../sampleServers/stdinEchoServer.js',
        '--enable-stdin',
      ]);

      expect(result.stdoutAsString()).toContain('Started');

      // Verify it appears in list
      const listResult = await workspace.runCli(['list']);
      expect(listResult.stdoutAsString()).toContain('transient-stdin');
    });

    it('should receive stdin messages for transient process', async () => {
      // Start a transient process with stdin enabled
      await workspace.runCli([
        'start',
        'transient-stdin-test',
        '--shell',
        'node ../../sampleServers/stdinEchoServer.js',
        '--enable-stdin',
      ]);
      await workspace.runCli(['wait-for-log', 'transient-stdin-test', '--message', 'Stdin echo server started']);

      // Send a message via the database
      createStdinMessage({
        commandName: 'transient-stdin-test',
        projectDir: workspace.dbDir,
        data: 'Hello transient!\n',
      });

      // Wait for the message to be received
      await workspace.runCli(['wait-for-log', 'transient-stdin-test', '--message', '[RECEIVED] Hello transient!']);

      // Verify the message appears in logs
      const logsResult = await workspace.runCli(['logs', 'transient-stdin-test']);
      expect(logsResult.stdoutAsString()).toContain('[RECEIVED] Hello transient!');
    });
  });

  describe('stdin without enableStdin', () => {
    it('should not receive stdin messages when enableStdin is false', async () => {
      // Start a process without enableStdin
      await workspace.runCli([
        'start',
        'no-stdin',
        '--shell',
        'node ../../sampleServers/stdinEchoServer.js',
      ]);
      await workspace.runCli(['wait-for-log', 'no-stdin', '--message', 'Stdin echo server started']);

      // Send a message via the database (should be ignored)
      createStdinMessage({
        commandName: 'no-stdin',
        projectDir: workspace.dbDir,
        data: 'This should not be received\n',
      });

      // Wait a bit for any potential processing
      await new Promise(resolve => setTimeout(resolve, 1000));

      // Verify the message does NOT appear in logs
      const logsResult = await workspace.runCli(['logs', 'no-stdin']);
      expect(logsResult.stdoutAsString()).not.toContain('This should not be received');
    });
  });
});
