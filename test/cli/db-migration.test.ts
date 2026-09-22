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
        for (const suffix of ['', '-wal', '-shm']) {
            fs.rmSync(path.join(workspace.dbDir, `candle.db${suffix}`), { force: true });
        }

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
});
