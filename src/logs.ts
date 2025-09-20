import * as fs from 'fs';
import * as path from 'path';
import { ProcessLogType } from './logs/processLogs.ts';

let isLoggingEnabled: boolean | null = null;

function checkLoggingEnabled(): boolean {
    if (isLoggingEnabled === null) {
        isLoggingEnabled = process.env.CANDLE_ENABLE_LOGS === 'true' || process.env.CANDLE_ENABLE_LOGS === '1';
    }
    return isLoggingEnabled;
}

export function infoLog(...args: any[]): void {
    // console.log('info: ', ...args);
    if (!checkLoggingEnabled()) {
        return;
    }
    
    const argsStr = args.map(arg => {
        if (typeof arg === 'string')
            return arg;
        return JSON.stringify(arg);
    });

    const timestamp = new Date().toISOString();
    const logEntry = `[${timestamp}] ${argsStr.join(' ')}\n`;
    const logPath = path.join(process.cwd(), 'candle.log');

    try {
        fs.appendFileSync(logPath, logEntry);
    } catch (error) {
        // Silently fail if we can't write to log file
        console.error('Failed to write to log file:', error);
    }
}



function consoleLogStdout(format: 'pretty' | 'json', msg: string) {
    if (format === 'json') {
        console.log(JSON.stringify({ stdout: msg }));
    } else {
        console.log(msg);
    }
}

function consoleLogStderr(format: 'pretty' | 'json', msg: string) {
    if (format === 'json')
        console.log(JSON.stringify({ stderr: msg }));
    else
        console.error(msg);
}

export function consoleLogSystemMessage(format: 'pretty' | 'json', msg: string) {
    if (format === 'json')
        console.log(JSON.stringify({ message: msg }));
    else
        console.log(`[${msg}]`);
}

export function consoleLogRow(format: 'pretty' | 'json', row: any) {
    switch (row.log_type) {
        case ProcessLogType.stdout:
            consoleLogStdout(format, row.content);
            break;
        case ProcessLogType.stderr:
            consoleLogStderr(format, row.content);
            break;
        case ProcessLogType.process_exited:
        case ProcessLogType.process_start_failed:
            consoleLogSystemMessage(format, row.content);
            break;
        case ProcessLogType.process_start_initiated:
        case ProcessLogType.process_started:
            // Hide these messages.
            break;
    }
}

