import { consoleLogSystemMessage, consoleLogRow } from './logs.ts';
import { LogIterator } from './logs/LogIterator.ts';
import type { ProcessLog } from './logs/processLogs.ts';

const INITIAL_LOG_COUNT = 100;
const POLL_INTERVAL = 200;

interface WatchOptions {
    projectDir: string;
    commandName: string;
    exitAfterMs?: number; // Optional timeout to exit watching after a certain period
    consoleOutputFormat: 'pretty' | 'json'
}

export async function watchProcess(options: WatchOptions): Promise<void> {
    const { projectDir, commandName, exitAfterMs, consoleOutputFormat } = options;
    const logIterator = new LogIterator({
        projectDir,
        commandName,
        limitToLatestProcessLogs: true
    });
    let watching = true;
    
    // Set up timeout if exitAfterMs is provided
    let timeoutId: NodeJS.Timeout | null = null;
    if (exitAfterMs && exitAfterMs > 0) {
        timeoutId = setTimeout(() => {
            consoleLogSystemMessage(consoleOutputFormat, `Exiting watch mode after ${exitAfterMs}ms timeout`);
            watching = false;
        }, exitAfterMs);
    }
    
    // Handle signals to stop watching
    const stopWatching = () => {
        watching = false;
        if (timeoutId) {
            clearTimeout(timeoutId);
        }
    };
    
    process.on('SIGINT', stopWatching);
    process.on('SIGTERM', stopWatching);

    function printLogs(logs: ProcessLog[]): void {
        logs.forEach(log => {
            consoleLogRow(consoleOutputFormat, log);
        });
    }

    // Initial log fetch
    printLogs(logIterator.getNextLogs({ limit: INITIAL_LOG_COUNT }));
    let processIsStillRunning = true;
    
    while (watching) {
        printLogs(logIterator.getNextLogs({ }));
        
        // Wait before next poll
        await new Promise(resolve => setTimeout(resolve, POLL_INTERVAL));
    }
    
    // Clean up timeout if it exists
    if (timeoutId) {
        clearTimeout(timeoutId);
    }
    
    if (processIsStillRunning) {
        consoleLogSystemMessage(consoleOutputFormat, 'Stopped watching. Process is still running in background.');
    }

    process.removeListener('SIGINT', stopWatching);
    process.removeListener('SIGTERM', stopWatching);
}
