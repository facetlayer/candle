import { getDatabase } from './database.ts';
import { cleanupStaleProcesses } from './staleProcessCleanup.ts';
import { findConfigFile, getLogEvictionConfig, type ResolvedLogEvictionConfig } from '../configFile.ts';

const CLEANUP_INTERVAL_SECONDS = 10 * 60;

export function maybeRunCleanup(): void {
  const db = getDatabase();
  const now = Math.floor(Date.now() / 1000);
  const lastCleanup = db.get('select timestamp from process_last_cleanup');
  if (lastCleanup && now - lastCleanup.timestamp < CLEANUP_INTERVAL_SECONDS) {
    // Last cleanup is recent enough.
    return;
  }

  // Try to load config for eviction settings
  let evictionConfig: ResolvedLogEvictionConfig;
  try {
    const configResult = findConfigFile(process.cwd());
    evictionConfig = getLogEvictionConfig(configResult?.config);
  } catch {
    // No config file found, use defaults
    evictionConfig = getLogEvictionConfig();
  }

  runCleanup(evictionConfig);
}

export function runCleanup(evictionConfig: ResolvedLogEvictionConfig): void {
  const db = getDatabase();
  const now = Math.floor(Date.now() / 1000);

  // Time-based eviction: delete logs older than maxRetentionSeconds
  const logCutoff = now - evictionConfig.maxRetentionSeconds;
  db.run('delete from process_output where timestamp < ?', [logCutoff]);

  // Remove database entries for processes that are no longer alive
  cleanupStaleProcesses();

  // Per-service eviction: keep only maxLogsPerService logs per (project_dir, command_name)
  // Find all distinct service keys that have more logs than the limit
  const services = db.list(
    `select project_dir, command_name, count(*) as log_count
     from process_output
     group by project_dir, command_name
     having count(*) > ?`,
    [evictionConfig.maxLogsPerService]
  );

  for (const service of services) {
    // Find the id threshold: keep the newest maxLogsPerService logs
    const cutoffRow = db.get(
      `select id from process_output
       where project_dir = ? and command_name = ?
       order by timestamp desc, id desc
       limit 1 offset ?`,
      [service.project_dir, service.command_name, evictionConfig.maxLogsPerService]
    );

    if (cutoffRow) {
      db.run(
        `delete from process_output
         where project_dir = ? and command_name = ? and id <= ?`,
        [service.project_dir, service.command_name, cutoffRow.id]
      );
    }
  }

  db.run('vacuum');
  db.upsert('process_last_cleanup', {}, { timestamp: now });
}
