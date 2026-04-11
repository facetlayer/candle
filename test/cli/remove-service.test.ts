import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import * as fs from 'fs';
import * as path from 'path';
import { TestWorkspace } from './utils';

const workspace = new TestWorkspace('cli-remove-service');

describe('CLI Remove-Service Command', () => {
    let tempDir: string;

    beforeEach(() => {
        tempDir = path.join(workspace.dbDir, 'fixture-' + Date.now());
        fs.mkdirSync(tempDir, { recursive: true });
    });

    afterEach(() => {
        if (fs.existsSync(tempDir)) {
            fs.rmSync(tempDir, { recursive: true, force: true });
        }
    });

    describe('removing a service', () => {
        it('should remove a service from .candle.json', async () => {
            const configPath = path.join(tempDir, '.candle.json');
            fs.writeFileSync(configPath, JSON.stringify({
                services: [
                    { name: 'my-service', shell: 'npm run dev' },
                    { name: 'other-service', shell: 'npm start' },
                ],
            }, null, 2));

            const result = await workspace.runCli(['remove-service', 'my-service'], { cwd: tempDir });

            expect(result.stdoutAsString()).toContain('my-service');
            expect(result.stdoutAsString().toLowerCase()).toContain('removed');

            const config = JSON.parse(fs.readFileSync(configPath, 'utf8'));
            expect(config.services).toHaveLength(1);
            expect(config.services[0].name).toBe('other-service');
        });

        it('should remove the only service leaving an empty array', async () => {
            const configPath = path.join(tempDir, '.candle.json');
            fs.writeFileSync(configPath, JSON.stringify({
                services: [{ name: 'only-service', shell: 'echo hi' }],
            }, null, 2));

            await workspace.runCli(['remove-service', 'only-service'], { cwd: tempDir });

            const config = JSON.parse(fs.readFileSync(configPath, 'utf8'));
            expect(config.services).toHaveLength(0);
        });

        it('should work with .candle-setup.json', async () => {
            const configPath = path.join(tempDir, '.candle-setup.json');
            fs.writeFileSync(configPath, JSON.stringify({
                services: [{ name: 'my-service', shell: 'npm start' }],
            }, null, 2));

            await workspace.runCli(['remove-service', 'my-service'], { cwd: tempDir });

            const config = JSON.parse(fs.readFileSync(configPath, 'utf8'));
            expect(config.services).toHaveLength(0);
        });
    });

    describe('error cases', () => {
        it('should error when service name not found', async () => {
            const configPath = path.join(tempDir, '.candle.json');
            fs.writeFileSync(configPath, JSON.stringify({
                services: [{ name: 'existing', shell: 'echo hi' }],
            }, null, 2));

            const result = await workspace.runCli(['remove-service', 'nonexistent'], { cwd: tempDir, ignoreExitCode: true });

            expect(result.failed()).toBe(true);
            expect(result.stderrAsString()).toContain('nonexistent');
        });

        it('should error when name is missing', async () => {
            const configPath = path.join(tempDir, '.candle.json');
            fs.writeFileSync(configPath, JSON.stringify({ services: [] }, null, 2));

            const result = await workspace.runCli(['remove-service'], { cwd: tempDir, ignoreExitCode: true });

            expect(result.failed()).toBe(true);
        });

        it('should error when no config file exists', async () => {
            const isolatedTempDir = path.join('/tmp', 'candle-remove-service-test-' + Date.now());
            fs.mkdirSync(isolatedTempDir, { recursive: true });

            try {
                const result = await workspace.runCli(['remove-service', 'my-service'], { cwd: isolatedTempDir, ignoreExitCode: true });
                expect(result.failed()).toBe(true);
            } finally {
                if (fs.existsSync(isolatedTempDir)) {
                    fs.rmSync(isolatedTempDir, { recursive: true, force: true });
                }
            }
        });
    });

    describe('output format', () => {
        it('should have success message', async () => {
            const configPath = path.join(tempDir, '.candle.json');
            fs.writeFileSync(configPath, JSON.stringify({
                services: [{ name: 'my-service', shell: 'npm start' }],
            }, null, 2));

            const result = await workspace.runCli(['remove-service', 'my-service'], { cwd: tempDir });

            expect(result.stdoutAsString()).toContain('my-service');
        });

        it('should have minimal stderr on success', async () => {
            const configPath = path.join(tempDir, '.candle.json');
            fs.writeFileSync(configPath, JSON.stringify({
                services: [{ name: 'my-service', shell: 'npm start' }],
            }, null, 2));

            const result = await workspace.runCli(['remove-service', 'my-service'], { cwd: tempDir });

            expect(result.stderrAsString()).toBe('');
        });
    });

    describe('preserves other config', () => {
        it('should preserve non-service config fields', async () => {
            const configPath = path.join(tempDir, '.candle.json');
            fs.writeFileSync(configPath, JSON.stringify({
                services: [{ name: 'my-service', shell: 'npm start' }],
                logEviction: { maxLogsPerService: 500, maxRetentionSeconds: 3600 },
            }, null, 2));

            await workspace.runCli(['remove-service', 'my-service'], { cwd: tempDir });

            const config = JSON.parse(fs.readFileSync(configPath, 'utf8'));
            expect(config.services).toHaveLength(0);
            expect(config.logEviction.maxLogsPerService).toBe(500);
        });
    });
});
