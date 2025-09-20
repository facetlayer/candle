import { describe, it, expect, beforeAll } from 'vitest';
import { spawn } from 'child_process';
import { clearTestData, getTestDataDirectory, getCandleBinPath } from './utils';

describe('Simple Candle Test', () => {
    beforeAll(() => {
        clearTestData('simple');
    });

    it('should show help when candle is run', async () => {
        const candlePath = getCandleBinPath();
        
        const result = await new Promise<{stdout: string, stderr: string, code: number}>((resolve) => {
            const proc = spawn('node', ['.', '--help'], { cwd: candlePath });
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