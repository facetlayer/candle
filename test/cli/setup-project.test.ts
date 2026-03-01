import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import * as fs from 'fs';
import * as path from 'path';
import { TestWorkspace } from './utils';

const workspace = new TestWorkspace('cli-setup-project');

describe('CLI setup-project Command', () => {
    let tempDir: string;

    beforeEach(() => {
        tempDir = path.join('/tmp', 'candle-setup-project-test-' + Date.now());
        fs.mkdirSync(tempDir, { recursive: true });
    });

    afterEach(() => {
        if (fs.existsSync(tempDir)) {
            fs.rmSync(tempDir, { recursive: true, force: true });
        }
    });

    it('should create .candle.json when none exists', async () => {
        const configPath = path.join(tempDir, '.candle.json');
        expect(fs.existsSync(configPath)).toBe(false);

        const result = await workspace.runCli(['setup-project'], { cwd: tempDir });

        expect(fs.existsSync(configPath)).toBe(true);
        expect(result.stdoutAsString()).toContain('.candle.json');
    });

    it('should create valid JSON with empty services array', async () => {
        await workspace.runCli(['setup-project'], { cwd: tempDir });

        const configPath = path.join(tempDir, '.candle.json');
        const config = JSON.parse(fs.readFileSync(configPath, 'utf8'));
        expect(config).toEqual({ services: [] });
    });

    it('should print success message', async () => {
        const result = await workspace.runCli(['setup-project'], { cwd: tempDir });

        expect(result.stdoutAsString()).toContain('Created');
        expect(result.stdoutAsString()).toContain('.candle.json');
    });

    it('should not overwrite when .candle.json already exists in current dir', async () => {
        const configPath = path.join(tempDir, '.candle.json');
        const existingConfig = { services: [{ name: 'existing', shell: 'echo hi' }] };
        fs.writeFileSync(configPath, JSON.stringify(existingConfig, null, 2));

        const result = await workspace.runCli(['setup-project'], { cwd: tempDir });

        expect(result.stdoutAsString()).toContain('already exists');

        // Verify original content preserved
        const config = JSON.parse(fs.readFileSync(configPath, 'utf8'));
        expect(config.services).toHaveLength(1);
        expect(config.services[0].name).toBe('existing');
    });

    it('should not create when .candle.json exists in parent dir', async () => {
        const parentConfig = path.join(tempDir, '.candle.json');
        fs.writeFileSync(parentConfig, JSON.stringify({ services: [] }, null, 2));

        const childDir = path.join(tempDir, 'subdir');
        fs.mkdirSync(childDir, { recursive: true });

        const result = await workspace.runCli(['setup-project'], { cwd: childDir });

        expect(result.stdoutAsString()).toContain('already exists');
        expect(fs.existsSync(path.join(childDir, '.candle.json'))).toBe(false);
    });

    it('should exit quickly', async () => {
        const startTime = Date.now();
        await workspace.runCli(['setup-project'], { cwd: tempDir });
        const elapsed = Date.now() - startTime;

        expect(elapsed).toBeLessThan(2000);
    });
});
