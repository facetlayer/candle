import { describe, it, expect, afterAll } from 'vitest';
import { DatabaseSync } from 'node:sqlite';
import * as path from 'path';
import { TestWorkspace } from './utils';

const workspace = new TestWorkspace('cli-log-collector-cleanup');

function isProcessAlive(pid: number): boolean {
  try {
    process.kill(pid, 0);
    return true;
  } catch {
    return false;
  }
}

function getLogCollectorPid(dbDir: string, commandName: string): number | null {
  const dbPath = path.join(dbDir, 'candle.db');
  const db = new DatabaseSync(dbPath);
  const row = db.prepare(
    'select log_collector_pid from processes where command_name = ? and killed_at is null'
  ).get(commandName) as { log_collector_pid: number } | undefined;
  db.close();
  return row?.log_collector_pid ?? null;
}

describe('log-collector cleanup after service kill', () => {
  afterAll(() => workspace.cleanup());

  it('should exit the log-collector process after the service is killed', async () => {
    // Start a service
    await workspace.runCli(['start', 'echo']);
    await workspace.runCli(['wait-for-log', 'echo', '--message', 'Echo server started']);

    // Read the log_collector_pid from the database
    const logCollectorPid = getLogCollectorPid(workspace.dbDir, 'echo');
    expect(logCollectorPid).not.toBeNull();
    expect(isProcessAlive(logCollectorPid!)).toBe(true);

    // Kill the service
    await workspace.runCli(['kill', 'echo']);

    // Wait for the log-collector to notice the exit and shut down
    const deadline = Date.now() + 5000;
    while (isProcessAlive(logCollectorPid!) && Date.now() < deadline) {
      await new Promise(resolve => setTimeout(resolve, 100));
    }

    expect(isProcessAlive(logCollectorPid!)).toBe(false);
  });
});
