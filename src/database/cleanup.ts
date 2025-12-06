import { getDatabase } from './database.ts';

const MAX_LOG_RETENTION_SECONDS = 24 * 60 * 60;
const CLEANUP_INTERVAL_SECONDS = 10 * 60;

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
  db.run('vacuum');

  db.upsert('process_last_cleanup', {}, { timestamp: now });
}
