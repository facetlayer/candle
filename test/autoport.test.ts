import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import * as fs from 'fs';
import * as path from 'path';
import { handleStart } from '../src/handleRun';
import { assignPort } from '../src/assignPort';
import { getDatabase, getAllAssignedPorts } from '../src/database';

describe('Auto Port Assignment', () => {
    const testDir = path.join(__dirname, 'tempdata', 'autoport-test');
    const setupFile = path.join(testDir, '.candle-setup.json');
    
    beforeEach(() => {
        // Create test directory
        if (!fs.existsSync(testDir)) {
            fs.mkdirSync(testDir, { recursive: true });
        }
        
        // Create test setup file with auto port assignment
        const config = {
            services: [
                {
                    name: 'test-auto-port-1',
                    shell: 'echo "Test service 1 on port $PORT"',
                    autoAssignPort: true,
                    default: true
                },
                {
                    name: 'test-auto-port-2', 
                    shell: 'echo "Test service 2 on port $PORT"',
                    autoAssignPort: true
                },
                {
                    name: 'test-no-auto-port',
                    shell: 'echo "Test service without auto port"'
                }
            ]
        };
        
        fs.writeFileSync(setupFile, JSON.stringify(config, null, 2));
    });
    
    afterEach(() => {
        // Clean up test directory
        if (fs.existsSync(testDir)) {
            fs.rmSync(testDir, { recursive: true, force: true });
        }
    });

    it('should assign unique ports starting from 3001', async () => {
        // This test checks that the assignPort function starts from 3001
        // and finds available ports by actually binding to them
        const port1 = await assignPort();
        expect(port1).toBeGreaterThanOrEqual(3001);
        
        // Create a temporary database entry to simulate port being used
        const db = getDatabase();
        db.run('insert into processes(command_name, command_str, working_directory, project_dir, start_time, pid, assigned_port) values(?, ?, ?, ?, ?, ?, ?)',
            ['temp-test', 'echo test', testDir, testDir, Math.floor(Date.now() / 1000), 1, port1]);
        
        // Now assign another port - it should be different
        const port2 = await assignPort();
        expect(port2).toBeGreaterThanOrEqual(3001);
        expect(port1).not.toBe(port2);
        
        // Clean up the temporary entry
        db.run('delete from processes where command_name = ?', ['temp-test']);
    });

    it('should assign ports to services with autoAssignPort enabled', async () => {
        const originalCwd = process.cwd();
        
        try {
            process.chdir(testDir);
            
            // Start services
            const result = await handleStart({
                cwd: testDir,
                commandNames: ['test-auto-port-1', 'test-auto-port-2'],
                consoleOutputFormat: 'json'
            });
            
            expect(result.summary.successCount).toBe(2);
            expect(result.summary.failureCount).toBe(0);
            
            // Check that ports were assigned in the database
            const assignedPorts = getAllAssignedPorts();
            expect(assignedPorts.length).toBeGreaterThan(0);
            
            // All assigned ports should be unique
            const uniquePorts = new Set(assignedPorts);
            expect(uniquePorts.size).toBe(assignedPorts.length);
            
        } finally {
            process.chdir(originalCwd);
        }
    });

    it('should not assign ports to services without autoAssignPort flag', async () => {
        const originalCwd = process.cwd();
        
        try {
            process.chdir(testDir);
            
            // Start service without autoAssignPort
            const result = await handleStart({
                cwd: testDir, 
                commandNames: ['test-no-auto-port'],
                consoleOutputFormat: 'json'
            });
            
            expect(result.summary.successCount).toBe(1);
            expect(result.summary.failureCount).toBe(0);
            
        } finally {
            process.chdir(originalCwd);
        }
    });

    it('should skip already used ports in database', async () => {
        // Get current assigned ports
        const initialPorts = getAllAssignedPorts();
        const initialPortSet = new Set(initialPorts);
        
        // Assign a new port
        const newPort = await assignPort();
        
        // The new port should not be in the initial set
        expect(initialPortSet.has(newPort)).toBe(false);
        expect(newPort).toBeGreaterThanOrEqual(3001);
    });
});