import { describe, it, expect } from 'vitest';
import { spawn } from 'child_process';
import { getCandleSpawn } from './TestWorkspace';

describe('Simple Candle Test', () => {
    it('should show help when candle is run', async () => {
        const { cmd, baseArgs } = getCandleSpawn();

        const result = await new Promise<{stdout: string, stderr: string, code: number}>((resolve) => {
            const proc = spawn(cmd, [...baseArgs, '--help']);
            let stdout = '';
            let stderr = '';

            proc.stdout.on('data', (data) => stdout += data);
            proc.stderr.on('data', (data) => stderr += data);

            proc.on('close', (code) => {
                resolve({ stdout, stderr, code: code || 0 });
            });
        });

        expect(result.stdout).toContain('Process Management:');
        expect(result.stdout).toContain('run');
        expect(result.stdout).toContain('kill');
        expect(result.code).toBe(0);
    });
});
