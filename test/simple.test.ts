import { describe, it, expect } from 'vitest';
import { spawn } from 'child_process';
import { getCliPath } from './utils';

describe('Simple Candle Test', () => {
    it('should show help when candle is run', async () => {
        const cliPath = getCliPath();

        const result = await new Promise<{stdout: string, stderr: string, code: number}>((resolve) => {
            const proc = spawn('node', [cliPath, '--help']);
            let stdout = '';
            let stderr = '';

            proc.stdout.on('data', (data) => stdout += data);
            proc.stderr.on('data', (data) => stderr += data);

            proc.on('close', (code) => {
                resolve({ stdout, stderr, code: code || 0 });
            });
        });

        expect(result.stdout).toContain('Commands:');
        expect(result.stdout).toContain('run');
        expect(result.stdout).toContain('kill');
        expect(result.code).toBe(0);
    });
});
