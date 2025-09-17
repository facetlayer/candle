import * as Db from './database/database.ts';
import { findConfigFile, getAllServiceNames, getServiceCwd } from './configFile.ts';
import * as path from 'path';
import { findAllProcesses, findProcessesByProjectDir } from './database/processTable.ts';

export interface ListOutput {
    processes: {
        command: string;
        workingDir: string;
        uptime: string;
        pid: number;
        status: string;
        serviceName: string;
    }[];
    showAll?: boolean;
    message?: string;
}

export async function handleList(options?: { showAll?: boolean }): Promise<ListOutput> {

    const { projectDir } = findConfigFile(process.cwd());

    if (options?.showAll) {
        const processEntries = findAllProcesses();
        
        const processes = processEntries.map(processEntry => {
            return {
                serviceName: processEntry.command_name,
                command: processEntry.command_name,
                workingDir: processEntry.project_dir,
                uptime: formatUptime(Date.now() - processEntry.start_time * 1000),
                pid: processEntry.pid,
                status: 'RUNNING'
            };
        });
        return { processes };
    } else {
        const processEntries = findProcessesByProjectDir(projectDir);

        // TODO: List the 'not running' processes that are in the config file.
        const processes = processEntries.map(processEntry => {
            return {
                serviceName: processEntry.command_name,
                command: processEntry.command_name,
                workingDir: processEntry.project_dir,
                uptime: formatUptime(Date.now() - processEntry.start_time * 1000),
                pid: processEntry.pid,
                status: 'RUNNING'
            };
        });
        return { processes };
    }
}

function formatUptime(seconds: number): string {
    const days = Math.floor(seconds / 86400);
    const hours = Math.floor((seconds % 86400) / 3600);
    const minutes = Math.floor((seconds % 3600) / 60);
    const secs = seconds % 60;

    const parts = [];
    if (days > 0) parts.push(`${days}d`);
    if (hours > 0) parts.push(`${hours}h`);
    if (minutes > 0) parts.push(`${minutes}m`);
    if (secs > 0 || parts.length === 0) parts.push(`${secs}s`);

    return parts.join(' ');
}

export function printListOutput(output: ListOutput): void {
    if (output.message) {
        console.log(output.message);
        return;
    }

    if (output.processes.length === 0) {
        console.log('No active processes found.');
        return;
    }

    const headers = ['NAME', 'STATUS', 'PID', 'UPTIME', 'COMMAND', 'DIRECTORY'];
    const rows = output.processes.map(process => [
        process.serviceName,
        process.status,
        process.pid > 0 ? process.pid.toString() : '-',
        process.uptime,
        process.command,
        process.workingDir
    ]);

    const columnWidths = headers.map((header, i) => 
        Math.max(header.length, ...rows.map(row => row[i].length))
    );

    const formatRow = (row: string[]) => 
        row.map((cell, i) => cell.padEnd(columnWidths[i])).join('  ');

    console.log(formatRow(headers));
    console.log(columnWidths.map(width => '-'.repeat(width)).join('  '));
    
    for (const row of rows) {
        console.log(formatRow(row));
    }
}

