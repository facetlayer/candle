import { DatabaseLoader, SqliteDatabase } from '@facetlayer/sqlite-wrapper';
import { Stream } from '@facetlayer/streams'
import * as Path from 'path';
import * as os from 'os';
import * as fs from 'fs';

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
            killed_at integer
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
        `create index idx_process_output_command_name on process_output(command_name)`,
        `create index idx_process_output_project_dir on process_output(project_dir)`,
    ]
};

let _db: SqliteDatabase;

function getStateDirectory(): string {
    // Check for environment variable first
    if (process.env.CANDLE_DATABASE_DIR) {
        return process.env.CANDLE_DATABASE_DIR;
    }
    
    // Default to ~/.candle/
    return Path.join(os.homedir(), '.candle');
}

export function getDatabase({overrideDirectory}: {overrideDirectory?: string} = {}): SqliteDatabase {
    if (!_db) {
        const stateDir = overrideDirectory ?? getStateDirectory();
        
        // Ensure the directory exists
        if (!fs.existsSync(stateDir)) {
            fs.mkdirSync(stateDir, { recursive: true });
        }
        
        const dbPath = Path.join(stateDir, 'candle.db');
        const loader = new DatabaseLoader({
            filename: dbPath,
            schema,
            logs: (new Stream()).logToConsole(),
        });
        _db = loader.load();
        
        // Set WAL mode and busy timeout for multi-process support
        _db.run('PRAGMA journal_mode=WAL');
        _db.run('PRAGMA busy_timeout=30000');
    }
    return _db;
}


