import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import * as fs from 'fs';
import * as path from 'path';
import { TestWorkspace } from './utils';

const workspace = new TestWorkspace('cli-add-service');

describe('CLI Add-Service Command', () => {
    let tempDir: string;

    beforeEach(() => {
        // Create fresh temp directory for each test
        tempDir = path.join(workspace.dbDir, 'fixture-' + Date.now());
        fs.mkdirSync(tempDir, { recursive: true });
    });

    afterEach(() => {
        if (fs.existsSync(tempDir)) {
            fs.rmSync(tempDir, { recursive: true, force: true });
        }
    });

    describe('adding to existing config', () => {
        it('should add service to existing .candle-setup.json', async () => {
            const configPath = path.join(tempDir, '.candle-setup.json');
            fs.writeFileSync(configPath, JSON.stringify({ services: [] }, null, 2));

            const result = await workspace.runCli(['add-service', 'my-service', '--shell', 'npm run dev'], { cwd: tempDir });

            expect(result.stdoutAsString()).toContain("'my-service'");
            expect(result.stdoutAsString().toLowerCase()).toContain('added');

            // Verify file was updated
            const config = JSON.parse(fs.readFileSync(configPath, 'utf8'));
            expect(config.services).toHaveLength(1);
            expect(config.services[0].name).toBe('my-service');
            expect(config.services[0].shell).toBe('npm run dev');
        });

        it('should add service to existing .candle.json', async () => {
            const configPath = path.join(tempDir, '.candle.json');
            fs.writeFileSync(configPath, JSON.stringify({ services: [] }, null, 2));

            await workspace.runCli(['add-service', 'my-service', '--shell', 'node server.js'], { cwd: tempDir });

            // Verify file was updated
            const config = JSON.parse(fs.readFileSync(configPath, 'utf8'));
            expect(config.services).toHaveLength(1);
            expect(config.services[0].name).toBe('my-service');
        });

        it('should add to existing services array', async () => {
            const configPath = path.join(tempDir, '.candle-setup.json');
            fs.writeFileSync(
                configPath,
                JSON.stringify(
                    {
                        services: [{ name: 'existing', shell: 'echo existing' }],
                    },
                    null,
                    2
                )
            );

            await workspace.runCli(['add-service', 'new-service', '--shell', 'echo new'], { cwd: tempDir });

            const config = JSON.parse(fs.readFileSync(configPath, 'utf8'));
            expect(config.services).toHaveLength(2);
            expect(config.services[0].name).toBe('existing');
            expect(config.services[1].name).toBe('new-service');
        });
    });

    describe('creating new config', () => {
        it('should create .candle.json when no config exists', async () => {
            // Use a truly isolated temp directory outside of any project with .candle.json
            const isolatedTempDir = path.join('/tmp', 'candle-add-service-test-' + Date.now());
            fs.mkdirSync(isolatedTempDir, { recursive: true });

            try {
                const configPath = path.join(isolatedTempDir, '.candle.json');

                expect(fs.existsSync(configPath)).toBe(false);

                await workspace.runCli(['add-service', 'new-service', '--shell', 'node app.js'], { cwd: isolatedTempDir });

                expect(fs.existsSync(configPath)).toBe(true);

                const config = JSON.parse(fs.readFileSync(configPath, 'utf8'));
                expect(config.services).toHaveLength(1);
                expect(config.services[0].name).toBe('new-service');
                expect(config.services[0].shell).toBe('node app.js');
            } finally {
                // Clean up
                if (fs.existsSync(isolatedTempDir)) {
                    fs.rmSync(isolatedTempDir, { recursive: true, force: true });
                }
            }
        });
    });

    describe('--root option', () => {
        it('should add service with root option', async () => {
            const configPath = path.join(tempDir, '.candle-setup.json');
            fs.writeFileSync(configPath, JSON.stringify({ services: [] }, null, 2));

            fs.mkdirSync(path.join(tempDir, 'packages/backend'), { recursive: true });

            await workspace.runCli(['add-service', 'backend', '--shell', 'npm start', '--root', 'packages/backend'], { cwd: tempDir });

            const config = JSON.parse(fs.readFileSync(configPath, 'utf8'));
            expect(config.services[0].root).toBe('packages/backend');
        });

        it('should reject a root directory that does not exist', async () => {
            const configPath = path.join(tempDir, '.candle.json');
            fs.writeFileSync(configPath, JSON.stringify({ services: [] }, null, 2));

            const result = await workspace.runCli(['add-service', 'backend', '--shell', 'npm start', '--root', './sub'], {
                cwd: tempDir,
                ignoreExitCode: true,
            });

            expect(result.failed()).toBe(true);
            expect(result.stderrAsString()).toContain('Root directory does not exist');
            expect(result.stderrAsString()).toContain(path.join(tempDir, 'sub'));
            const config = JSON.parse(fs.readFileSync(configPath, 'utf8'));
            expect(config.services).toHaveLength(0);
        });
    });

    describe('name validation', () => {
        for (const badName of ['bad name!', 'a;b', '$(whoami)', 'x/y']) {
            it(`should reject ${JSON.stringify(badName)}`, async () => {
                const configPath = path.join(tempDir, '.candle.json');
                fs.writeFileSync(configPath, JSON.stringify({ services: [] }, null, 2));

                const result = await workspace.runCli(['add-service', badName, '--shell', 'npm start'], {
                    cwd: tempDir,
                    ignoreExitCode: true,
                });

                expect(result.failed()).toBe(true);
                expect(result.stderrAsString()).toContain('Invalid service name');
                const config = JSON.parse(fs.readFileSync(configPath, 'utf8'));
                expect(config.services).toHaveLength(0);
            });
        }

        it('should accept letters, digits, dashes, underscores and dots', async () => {
            const configPath = path.join(tempDir, '.candle.json');
            fs.writeFileSync(configPath, JSON.stringify({ services: [] }, null, 2));

            await workspace.runCli(['add-service', 'Web_2.api-v1', '--shell', 'npm start'], { cwd: tempDir });

            const config = JSON.parse(fs.readFileSync(configPath, 'utf8'));
            expect(config.services[0].name).toBe('Web_2.api-v1');
        });
    });

    describe('config file rewriting', () => {
        it('should end the written file with a newline', async () => {
            const configPath = path.join(tempDir, '.candle.json');
            fs.writeFileSync(configPath, JSON.stringify({ services: [] }, null, 2));

            await workspace.runCli(['add-service', 'api', '--shell', 'npm start'], { cwd: tempDir });

            expect(fs.readFileSync(configPath, 'utf8').endsWith('}\n')).toBe(true);
        });

        it('should preserve unknown per-service keys', async () => {
            const configPath = path.join(tempDir, '.candle.json');
            fs.writeFileSync(
                configPath,
                JSON.stringify({ services: [{ name: 'existing', shell: 'echo hi', env: { PORT: '3000' }, note: 'keep' }] }, null, 2)
            );

            const result = await workspace.runCli(['add-service', 'api', '--shell', 'npm start'], { cwd: tempDir });

            const config = JSON.parse(fs.readFileSync(configPath, 'utf8'));
            expect(config.services[0]).toEqual({ name: 'existing', shell: 'echo hi', env: { PORT: '3000' }, note: 'keep' });
            expect(config.services[1]).toEqual({ name: 'api', shell: 'npm start' });
            // Unknown keys still warn.
            expect(result.stderrAsString()).toContain('unknown key "env" in service "existing"');
        });
    });

    describe('unknown config keys', () => {
        it('should warn (not fail) on an unknown service key, suggesting the likely one', async () => {
            fs.writeFileSync(
                path.join(tempDir, '.candle.json'),
                JSON.stringify({ services: [{ name: 'web', shell: 'echo hi', cwd: 'sub' }], extra: 1 }, null, 2)
            );

            const result = await workspace.runCli(['list'], { cwd: tempDir });

            const stderr = result.stderrAsString();
            expect(stderr).toContain('Warning: unknown key "cwd" in service "web" in .candle.json (did you mean "root"?)');
            expect(stderr).toContain('Warning: unknown key "extra" in .candle.json');
            // Printed once, even though the config is read more than once.
            expect(stderr.split('"cwd"').length - 1).toBe(1);
            expect(result.stdoutAsString()).toContain('web');
        });
    });

    describe('missing required arguments', () => {
        it('should error when name is missing', async () => {
            const configPath = path.join(tempDir, '.candle-setup.json');
            fs.writeFileSync(configPath, JSON.stringify({ services: [] }, null, 2));

            const result = await workspace.runCli(['add-service'], { cwd: tempDir, ignoreExitCode: true });

            expect(result.failed()).toBe(true);
        });

        it('should error when shell is missing', async () => {
            const configPath = path.join(tempDir, '.candle-setup.json');
            fs.writeFileSync(configPath, JSON.stringify({ services: [] }, null, 2));

            const result = await workspace.runCli(['add-service', 'my-service'], { cwd: tempDir, ignoreExitCode: true });

            expect(result.failed()).toBe(true);
        });
    });

    describe('special characters', () => {
        it('should handle shell commands with spaces', async () => {
            const configPath = path.join(tempDir, '.candle-setup.json');
            fs.writeFileSync(configPath, JSON.stringify({ services: [] }, null, 2));

            await workspace.runCli(['add-service', 'my-service', '--shell', 'npm run start:dev'], { cwd: tempDir });

            const config = JSON.parse(fs.readFileSync(configPath, 'utf8'));
            expect(config.services[0].shell).toBe('npm run start:dev');
        });

        it('should handle service names with dashes', async () => {
            const configPath = path.join(tempDir, '.candle-setup.json');
            fs.writeFileSync(configPath, JSON.stringify({ services: [] }, null, 2));

            await workspace.runCli(['add-service', 'my-backend-service', '--shell', 'npm start'], { cwd: tempDir });

            const config = JSON.parse(fs.readFileSync(configPath, 'utf8'));
            expect(config.services[0].name).toBe('my-backend-service');
        });
    });

    describe('output format', () => {
        it('should have success message', async () => {
            const configPath = path.join(tempDir, '.candle-setup.json');
            fs.writeFileSync(configPath, JSON.stringify({ services: [] }, null, 2));

            const result = await workspace.runCli(['add-service', 'my-service', '--shell', 'npm start'], { cwd: tempDir });

            expect(result.stdoutAsString()).toContain('my-service');
        });

        it('should have minimal stderr on success', async () => {
            const configPath = path.join(tempDir, '.candle-setup.json');
            fs.writeFileSync(configPath, JSON.stringify({ services: [] }, null, 2));

            const result = await workspace.runCli(['add-service', 'my-service', '--shell', 'npm start'], { cwd: tempDir });

            expect(result.stderrAsString()).toBe('');
        });
    });

    describe('exit behavior', () => {
        it('should exit quickly', async () => {
            const configPath = path.join(tempDir, '.candle-setup.json');
            fs.writeFileSync(configPath, JSON.stringify({ services: [] }, null, 2));

            const startTime = Date.now();
            await workspace.runCli(['add-service', 'my-service', '--shell', 'npm start'], { cwd: tempDir });
            const elapsed = Date.now() - startTime;

            expect(elapsed).toBeLessThan(2000);
        });
    });
});
