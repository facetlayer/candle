import { getDatabase } from './database.ts';
import { cleanupStaleProcesses } from './staleProcessCleanup.ts';

const MAX_LOG_RETENTION_SECONDS = 24 * 60 * 60;
const CLEANUP_INTERVAL_SECONDS = 10 * 60;
const MAX_LOG_LINES_PER_SERVICE = 10000;

export function maybeRunCleanup(): void {
  const db = getDatabase();
  const now = Math.floor(Date.now() / 1000);
  const lastCleanup = db.get('select timestamp from process_last_cleanup');
  if (lastCleanup && now - lastCleanup.timestamp < CLEANUP_INTERVAL_SECONDS) {
    // Last cleanup is recent enough.
    return;
  }

  runCleanup();
}

function runCleanup(): void {
  const db = getDatabase();
  const now = Math.floor(Date.now() / 1000);

  // Keep process output logs for 24 hours
  const logCutoff = now - MAX_LOG_RETENTION_SECONDS;

  // Delete old process output logs
  db.run('delete from process_output where timestamp < ?', [logCutoff]);

  // Enforce per-service log limits
  evictExcessLogs();

  // Remove database entries for processes that are no longer alive
  cleanupStaleProcesses();

  db.run('vacuum');

  db.upsert('process_last_cleanup', {}, { timestamp: now });
}

function evictExcessLogs(): void {
  const db = getDatabase();

  // Get all unique (command_name, project_dir) combinations
  const services = db.list(
    'select distinct command_name, project_dir from process_output'
  );

  for (const service of services) {
    const { command_name, project_dir } = service;

    // Count logs for this service
    const countResult = db.get(
      'select count(*) as count from process_output where command_name = ? and project_dir = ?',
      [command_name, project_dir]
    );

    const logCount = countResult?.count || 0;

    if (logCount > MAX_LOG_LINES_PER_SERVICE) {
      const excessCount = logCount - MAX_LOG_LINES_PER_SERVICE;

      // Delete the oldest logs (lowest id means oldest)
      db.run(
        `delete from process_output
         where command_name = ? and project_dir = ?
         and id in (
           select id from process_output
           where command_name = ? and project_dir = ?
           order by id asc
           limit ?
         )`,
        [command_name, project_dir, command_name, project_dir, excessCount]
      );
    }
  }
}
