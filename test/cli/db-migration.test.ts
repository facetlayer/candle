import { describe, it, expect, beforeAll, afterAll } from 'vitest';
import * as fs from 'fs';
import * as path from 'path';
import { DatabaseSync } from 'node:sqlite';
import { TestWorkspace } from './utils';

const workspace = new TestWorkspace('cli-db-migration');

function columns(db: DatabaseSync, table: string): string[] {
    return (db.prepare(`PRAGMA table_info(${table})`).all() as { name: string }[]).map((r) => r.name);
}

describe('Database migration', () => {
    beforeAll(() => {
        fs.writeFileSync(path.join(workspace.dbDir, '.candle.json'), '{ "services": [] }\n');
    });

    afterAll(() => workspace.cleanup());

    it('upgrades a very old candle.db that lacks newer columns', async () => {
        removeDatabase();

        const old = new DatabaseSync(path.join(workspace.dbDir, 'candle.db'));
        old.exec(`
            create table processes(
                id integer primary key autoincrement,
                command_name text not null,
                project_dir text not null,
                pid integer not null,
                start_time integer not null,
                created_at integer not null default (strftime('%s', 'now'))
            );
            create table process_output(
                id integer primary key autoincrement,
                command_name text not null,
                project_dir text not null,
                content text,
                log_type integer not null
            );
            insert into process_output(command_name, project_dir, content, log_type)
                values('old', '/nowhere', 'ancient line', 1);
        `);
        old.close();

        const result = await workspace.runCli(['list']);
        expect(result.stderrAsString()).toBe('');

        const db = new DatabaseSync(path.join(workspace.dbDir, 'candle.db'));
        try {
            expect(columns(db, 'processes')).toEqual(
                expect.arrayContaining(['log_collector_pid', 'killed_at', 'shell', 'root'])
            );
            expect(columns(db, 'process_output')).toContain('timestamp');
            const row = db.prepare('select content from process_output where command_name = ?').get('old') as
                | { content: string }
                | undefined;
            expect(row?.content).toBe('ancient line');
        } finally {
            db.close();
        }
    });

    it('moves logs from the process_output table into services + log_lines', async () => {
        removeDatabase();
        const projectDir = workspace.dbDir;

        // The log table as the release before log_lines wrote it, with two runs of 'api'.
        const old = new DatabaseSync(path.join(workspace.dbDir, 'candle.db'));
        old.exec(`
            create table process_output(
                id integer primary key autoincrement,
                command_name text not null,
                project_dir text not null,
                content text,
                log_type integer not null,
                timestamp integer not null default (strftime('%s', 'now')),
                run_id integer
            );
            create index idx_process_output_lookup on process_output(project_dir, command_name, timestamp desc, id desc);
        `);
        const insert = old.prepare(
            'insert into process_output(command_name, project_dir, content, log_type, run_id) values(?, ?, ?, ?, ?)'
        );
        insert.run('api', projectDir, null, 3, 1);
        insert.run('api', projectDir, 'FATAL: first run crashed', 2, 1);
        insert.run('api', projectDir, 'Process exited with code 1', 6, 1);
        insert.run('api', projectDir, null, 3, 4);
        insert.run('api', projectDir, 'second run output', 1, 4);
        old.close();

        const latest = await workspace.runCli(['logs', 'api']);
        expect(latest.stderrAsString()).toBe('');
        expect(latest.stdoutAsString()).toContain('second run output');
        expect(latest.stdoutAsString()).not.toContain('FATAL');

        const previous = await workspace.runCli(['logs', 'api', '--previous']);
        expect(previous.stdoutAsString()).toContain('FATAL: first run crashed');

        const db = new DatabaseSync(path.join(workspace.dbDir, 'candle.db'));
        try {
            const kind = db.prepare("select type from sqlite_master where name = 'process_output'").get() as {
                type: string;
            };
            expect(kind.type).toBe('view');
            const counts = db
                .prepare('select (select count(*) from log_lines) as lines, (select count(*) from services) as services')
                .get() as { lines: number; services: number };
            expect(counts).toEqual({ lines: 5, services: 1 });
        } finally {
            db.close();
        }
    });
});

function removeDatabase() {
    for (const suffix of ['', '-wal', '-shm']) {
        fs.rmSync(path.join(workspace.dbDir, `candle.db${suffix}`), { force: true });
    }
}
