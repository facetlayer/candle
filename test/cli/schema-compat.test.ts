import { describe, it, expect, beforeEach, afterAll } from 'vitest';
import { TestWorkspace } from './utils';
import * as path from 'path';
import * as fs from 'fs';
import { execSync } from 'child_process';

const workspace = new TestWorkspace('schema-compat');

const frozenDbZip = path.join(workspace.dbDir, 'frozen-v1.0.0.db.zip');

describe('Schema Compatibility', () => {
    afterAll(() => workspace.cleanup());

    beforeEach(() => {
        // Remove any existing database before each test
        const dbPath = path.join(workspace.dbDir, 'candle.db');
        const walPath = path.join(workspace.dbDir, 'candle.db-wal');
        const shmPath = path.join(workspace.dbDir, 'candle.db-shm');

        for (const f of [dbPath, walPath, shmPath]) {
            if (fs.existsSync(f)) fs.unlinkSync(f);
        }

        // Unzip the frozen database into the workspace
        execSync(`unzip -o ${frozenDbZip} -d ${workspace.dbDir}`);
    });

    it('should open a v1.0.0 database with no warnings', async () => {
        const result = await workspace.runCli(['list']);

        // The list command should succeed and show the expected table format
        expect(result.stdoutAsString()).toContain('NAME');
        expect(result.stdoutAsString()).toContain('STATUS');

        // There should be no warnings printed to stderr about schema drift
        const stderr = result.stderrAsString();
        expect(stderr).toBe('');
    });

    it('should run start and logs with no warnings on a v1.0.0 database', async () => {
        const startResult = await workspace.runCli(['start', 'echo']);
        expect(startResult.stderrAsString()).toBe('');

        const logsResult = await workspace.runCli(['logs', 'echo']);
        expect(logsResult.stderrAsString()).toBe('');
    });
});
