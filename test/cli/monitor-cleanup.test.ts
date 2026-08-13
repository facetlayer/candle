import { describe, it, expect, afterAll } from 'vitest';
import { DatabaseSync } from 'node:sqlite';
import * as path from 'path';
import { TestWorkspace } from './utils';

const workspace = new TestWorkspace('cli-monitor-cleanup');

function isProcessAlive(pid: number): boolean {
  try {
    process.kill(pid, 0);
    return true;
  } catch {
    return false;
  }
}

function getMonitorPid(dbDir: string, commandName: string): number | null {
  const dbPath = path.join(dbDir, 'candle.db');
  const db = new DatabaseSync(dbPath);
  const row = db.prepare(
    'select log_collector_pid from processes where command_name = ? and killed_at is null'
  ).get(commandName) as { log_collector_pid: number } | undefined;
  db.close();
  return row?.log_collector_pid ?? null;
}

describe('monitor process cleanup after service kill', () => {
  afterAll(() => workspace.cleanup());

  it('should exit the monitor process after the service is killed', async () => {
    // Start a service
    await workspace.runCli(['start', 'echo']);
    await workspace.runCli(['wait-for-log', 'echo', '--message', 'Echo server started']);

    // Read the monitor's pid from the database (the column keeps its legacy name)
    const monitorPid = getMonitorPid(workspace.dbDir, 'echo');
    expect(monitorPid).not.toBeNull();
    expect(isProcessAlive(monitorPid!)).toBe(true);

    // Kill the service
    await workspace.runCli(['kill', 'echo']);

    // Wait for the monitor to notice the exit and shut down
    const deadline = Date.now() + 5000;
    while (isProcessAlive(monitorPid!) && Date.now() < deadline) {
      await new Promise(resolve => setTimeout(resolve, 100));
    }

    expect(isProcessAlive(monitorPid!)).toBe(false);
  });
});
