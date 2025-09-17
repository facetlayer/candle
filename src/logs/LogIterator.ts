import { getProcessLogs } from "./processLogs.ts";
import type { LogSearchOptions } from "./processLogs.ts";
import type { ProcessLog } from "./processLogs.ts";

export class LogIterator {
    lastSeenLogId: number | null = null;
    options: LogSearchOptions;

    constructor(options: LogSearchOptions) {
        this.options = options;
    }

    getNextLogs(options: Partial<LogSearchOptions> = {}): ProcessLog[] {
        const fullOptions: LogSearchOptions = {
            ...this.options,
            ...options,
        };

        if (this.lastSeenLogId !== null) {
            fullOptions.afterLogId = this.lastSeenLogId;
        }

        const logs = getProcessLogs(fullOptions);

        if (logs.length > 0) {
            this.lastSeenLogId = logs[logs.length - 1].id;
        }
        return logs;
    }
}