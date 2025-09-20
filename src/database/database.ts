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
        /*
        `create table messages(
            id integer primary key autoincrement,
            sent_to text not null,
            message_json text not null,
            received_by_pid integer,
            received_at integer,
            created_at integer not null
        )`,
        */
        `create index idx_process_output_command_name on process_output(command_name)`,
        `create index idx_process_output_project_dir on process_output(project_dir)`,
        //`create index idx_processes_is_running on processes(is_running)`,
        //`create index idx_processes_pid on processes(pid)`
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


export function databaseCleanup(): void {
    const db = getDatabase();
    const now = Math.floor(Date.now() / 1000);
    
    // Keep failed processes for 24 hours so users can view failure logs
    const failedProcessCutoff = now - (24 * 60 * 60); // 24 hours
    
    // Keep completed processes for 4 hours
    const completedProcessCutoff = now - (4 * 60 * 60); // 4 hours
    
    // Keep process output logs for 24 hours (for both failed and completed processes)
    const logCutoff = now - (24 * 60 * 60); // 24 hours
    
    // Enforce max logs per process by keeping only the most recent logs
    /* TODO revisit - right now this query gets too slow
    db.run(`delete from process_output where id not in (
        select id from process_output po1 
        where (
            select count(*) from process_output po2 
            where po2.command_name = po1.command_name and po2.project_dir = po1.project_dir and po2.id >= po1.id
        ) <= ?
    )`, [MAX_LOGS_PER_PROCESS]);
    */
    
    // Delete old process output logs
    db.run('delete from process_output where timestamp < ?', [logCutoff]);
    
    // Delete old failed processes (after 24 hours)
    // (fixme)
    //db.run('delete from processes where is_running = ? and exit_code != 0 and created_at < ?', [RunningStatus.stopped, failedProcessCutoff]);
    
    // Delete old completed processes (after 4 hours)  
    //db.run('delete from processes where is_running = ? and exit_code = 0 and created_at < ?', [RunningStatus.stopped, completedProcessCutoff]);
    
    // Note: Running processes are never deleted by cleanup
    
    // Clean up orphaned logs (logs without corresponding processes)
    db.run(`delete from process_output 
            where (command_name, project_dir) not in (select command_name, project_dir from processes)`);
    
    db.run('vacuum');
}
