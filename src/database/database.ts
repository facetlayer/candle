import { DatabaseLoader, SqliteDatabase } from '@facetlayer/sqlite-wrapper';
import { Stream } from '@facetlayer/streams';
import * as fs from 'fs';
import * as Path from 'path';
import { getStateDirectory } from '../dirs.ts';

export const RunningStatus = {
  running: 1,
  stopped: 0,
} as const;

export type RunningStatus = (typeof RunningStatus)[keyof typeof RunningStatus];

const schema = {
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
    `create table reserved_ports(
            port integer primary key,
            project_dir text not null,
            service_name text,
            assigned_at integer not null default (strftime('%s', 'now'))
        )`,
    `create table next_port_to_try(
            id integer primary key check (id = 1),
            port integer not null
        )`,
    `create index idx_reserved_ports_project_dir on reserved_ports(project_dir)`,
    `create index idx_reserved_ports_service on reserved_ports(project_dir, service_name)`,
  ],
};

let _db: SqliteDatabase;

export function getDatabase({
  overrideDirectory,
}: { overrideDirectory?: string } = {}): SqliteDatabase {
  if (!_db) {
    const stateDir = overrideDirectory ?? getStateDirectory();

    if (!fs.existsSync(stateDir)) {
      fs.mkdirSync(stateDir, { recursive: true });
    }

    const dbPath = Path.join(stateDir, 'candle.db');
    const loader = new DatabaseLoader({
      filename: dbPath,
      schema,
      logs: new Stream().logToConsole(),
    });
    _db = loader.load();

    // Set WAL mode and busy timeout for multi-process support
    _db.run('PRAGMA journal_mode=WAL');
    _db.run('PRAGMA busy_timeout=30000');
  }
  return _db;
}
