import { describe, it, expect, beforeEach, vi } from 'vitest';
import * as fs from 'fs';
import * as os from 'os';
import * as path from 'path';
import { DatabaseLoader, type SqliteDatabase } from '@facetlayer/sqlite-wrapper';
import Database from 'better-sqlite3';
import { LOG_EVICTION_DEFAULTS } from '../../configFile.ts';

// Mock the database module before importing cleanup
let mockDb: SqliteDatabase;
vi.mock('../database.ts', () => ({
  getDatabase: () => mockDb,
}));

// Mock stale process cleanup to avoid side effects in unit tests
vi.mock('../staleProcessCleanup.ts', () => ({
  cleanupStaleProcesses: () => {},
}));

// Import after mock setup - use require-style to avoid top-level await
import { runCleanup } from '../cleanup.ts';

// Create a standalone test database
function createTestDatabase(dir: string): SqliteDatabase {
  if (!fs.existsSync(dir)) {
    fs.mkdirSync(dir, { recursive: true });
  }

  const dbPath = path.join(dir, 'candle.db');
  const loader = new DatabaseLoader({
    filename: dbPath,
    schema: {
      name: 'CandleDatabase',
      statements: [
        `create table processes(
            id integer primary key autoincrement,
            command_name text not null,
            project_dir text not null,
            pid integer not null,
            log_collector_pid integer,
            start_time integer not null,
            created_at integer not null default (strftime('%s', 'now')),
            killed_at integer,
            shell text,
            root text
        )`,
        `create table process_output(
            id integer primary key autoincrement,
            command_name text not null,
            project_dir text not null,
            content text,
            log_type integer not null,
            timestamp integer not null default (strftime('%s', 'now'))
        )`,
        `create table process_last_cleanup(
           timestamp integer not null
        )`,
        `create table stdin_messages(
            id integer primary key autoincrement,
            command_name text not null,
            project_dir text not null,
            data text not null,
            encoding text not null default 'utf8',
            created_at integer not null default (strftime('%s', 'now'))
        )`,
        `create index idx_process_output_command_name on process_output(command_name)`,
        `create index idx_process_output_project_dir on process_output(project_dir)`,
        `create index idx_process_output_lookup on process_output(project_dir, command_name, timestamp desc, id desc)`,
        `create index idx_stdin_messages_lookup on stdin_messages(project_dir, command_name, id)`,
      ],
    },
    logs: {
      info: () => {},
      warn: (msg) => console.warn(msg),
      error: (err) => console.error(err.errorMessage),
    },
    loadDatabase: (filename: string) => new Database(filename),
    migrationBehavior: 'safe-upgrades',
  });
  const db = loader.load();
  db.run('PRAGMA journal_mode=WAL');
  db.run('PRAGMA busy_timeout=30000');
  return db;
}

function insertLog(db: SqliteDatabase, commandName: string, projectDir: string, content: string, timestamp: number) {
  db.run(
    'insert into process_output(command_name, project_dir, content, log_type, timestamp) values(?, ?, ?, 1, ?)',
    [commandName, projectDir, content, timestamp]
  );
}

function getLogCount(db: SqliteDatabase, commandName?: string, projectDir?: string): number {
  if (commandName && projectDir) {
    const row = db.get(
      'select count(*) as count from process_output where command_name = ? and project_dir = ?',
      [commandName, projectDir]
    );
    return row.count;
  }
  const row = db.get('select count(*) as count from process_output');
  return row.count;
}

function getLogContents(db: SqliteDatabase, commandName: string, projectDir: string): string[] {
  const rows = db.list(
    'select content from process_output where command_name = ? and project_dir = ? order by timestamp asc, id asc',
    [commandName, projectDir]
  );
  return rows.map((r: any) => r.content);
}

const now = Math.floor(Date.now() / 1000);

describe('runCleanup', () => {
  beforeEach(() => {
    const testDir = fs.mkdtempSync(path.join(os.tmpdir(), 'candle-cleanup-test-'));
    mockDb = createTestDatabase(testDir);
  });

  it('should delete logs older than maxRetentionSeconds', () => {
    const oldTimestamp = now - 48 * 60 * 60; // 48 hours ago
    const recentTimestamp = now - 60; // 1 minute ago

    insertLog(mockDb, 'web', '/project', 'old log', oldTimestamp);
    insertLog(mockDb, 'web', '/project', 'recent log', recentTimestamp);

    expect(getLogCount(mockDb)).toBe(2);

    runCleanup({
      maxLogsPerService: 10000,
      maxRetentionSeconds: LOG_EVICTION_DEFAULTS.maxRetentionSeconds,
    });

    expect(getLogCount(mockDb)).toBe(1);
    expect(getLogContents(mockDb, 'web', '/project')).toEqual(['recent log']);
  });

  it('should respect custom maxRetentionSeconds', () => {
    const twoHoursAgo = now - 2 * 60 * 60;
    const thirtyMinutesAgo = now - 30 * 60;

    insertLog(mockDb, 'web', '/project', 'two hours old', twoHoursAgo);
    insertLog(mockDb, 'web', '/project', 'thirty minutes old', thirtyMinutesAgo);

    runCleanup({
      maxLogsPerService: 10000,
      maxRetentionSeconds: 3600,
    });

    expect(getLogCount(mockDb)).toBe(1);
    expect(getLogContents(mockDb, 'web', '/project')).toEqual(['thirty minutes old']);
  });

  it('should enforce per-service log limits', () => {
    for (let i = 0; i < 10; i++) {
      insertLog(mockDb, 'web', '/project', `log ${i}`, now - (10 - i));
    }

    expect(getLogCount(mockDb, 'web', '/project')).toBe(10);

    runCleanup({
      maxLogsPerService: 5,
      maxRetentionSeconds: 86400,
    });

    expect(getLogCount(mockDb, 'web', '/project')).toBe(5);
    expect(getLogContents(mockDb, 'web', '/project')).toEqual([
      'log 5', 'log 6', 'log 7', 'log 8', 'log 9',
    ]);
  });

  it('should apply per-service limits independently per service', () => {
    for (let i = 0; i < 8; i++) {
      insertLog(mockDb, 'api', '/project', `api log ${i}`, now - (10 - i));
    }

    for (let i = 0; i < 3; i++) {
      insertLog(mockDb, 'web', '/project', `web log ${i}`, now - (10 - i));
    }

    expect(getLogCount(mockDb, 'api', '/project')).toBe(8);
    expect(getLogCount(mockDb, 'web', '/project')).toBe(3);

    runCleanup({
      maxLogsPerService: 5,
      maxRetentionSeconds: 86400,
    });

    expect(getLogCount(mockDb, 'api', '/project')).toBe(5);
    expect(getLogContents(mockDb, 'api', '/project')).toEqual([
      'api log 3', 'api log 4', 'api log 5', 'api log 6', 'api log 7',
    ]);

    expect(getLogCount(mockDb, 'web', '/project')).toBe(3);
  });

  it('should apply per-service limits independently per project', () => {
    for (let i = 0; i < 8; i++) {
      insertLog(mockDb, 'web', '/project-a', `project-a log ${i}`, now - (10 - i));
    }
    for (let i = 0; i < 8; i++) {
      insertLog(mockDb, 'web', '/project-b', `project-b log ${i}`, now - (10 - i));
    }

    runCleanup({
      maxLogsPerService: 5,
      maxRetentionSeconds: 86400,
    });

    expect(getLogCount(mockDb, 'web', '/project-a')).toBe(5);
    expect(getLogCount(mockDb, 'web', '/project-b')).toBe(5);
  });

  it('should not evict logs when count is at the limit', () => {
    for (let i = 0; i < 5; i++) {
      insertLog(mockDb, 'web', '/project', `log ${i}`, now - (10 - i));
    }

    runCleanup({
      maxLogsPerService: 5,
      maxRetentionSeconds: 86400,
    });

    expect(getLogCount(mockDb, 'web', '/project')).toBe(5);
  });

  it('should combine time-based and count-based eviction', () => {
    insertLog(mockDb, 'web', '/project', 'old log', now - 48 * 60 * 60);
    for (let i = 0; i < 8; i++) {
      insertLog(mockDb, 'web', '/project', `recent log ${i}`, now - (10 - i));
    }

    runCleanup({
      maxLogsPerService: 5,
      maxRetentionSeconds: 86400,
    });

    expect(getLogCount(mockDb, 'web', '/project')).toBe(5);
    expect(getLogContents(mockDb, 'web', '/project')).toEqual([
      'recent log 3', 'recent log 4', 'recent log 5', 'recent log 6', 'recent log 7',
    ]);
  });

  it('should update the last cleanup timestamp', () => {
    runCleanup({
      maxLogsPerService: 1000,
      maxRetentionSeconds: 86400,
    });

    const lastCleanup = mockDb.get('select timestamp from process_last_cleanup');
    expect(lastCleanup).toBeDefined();
    expect(lastCleanup.timestamp).toBeGreaterThan(0);
    expect(Math.abs(lastCleanup.timestamp - now)).toBeLessThan(5);
  });
});
