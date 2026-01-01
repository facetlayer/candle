import { debugLog } from '../debug.ts';
import type { LogSearchOptions, ProcessLog } from './processLogs.ts';
import { getProcessLogs } from './processLogs.ts';

export class LogIterator {
  currentLogId: number | null = null;
  options: LogSearchOptions;

  constructor(options: LogSearchOptions) {
    this.options = options;
  }

  copy(): LogIterator {
    const copy = new LogIterator(this.options);
    copy.currentLogId = this.currentLogId;
    return copy;
  }

  /*
    Set the iterator position to use the most recent log message.
    (with these filters)
  */
  resetToLatestLogMessage() {
    this.currentLogId = null;

    const logs = getProcessLogs({
      ...this.options,
      limit: 1
    });

    // temp
    //console.log('[LogIterator] resetToLatestLogMessage', logs);

    if (logs.length > 0) {
      this.currentLogId = logs[0].id;
    }
  }

  /*
    Start an async iterator that will yield logs as they are found.
    (using the filter options)
  */
  async *it() {
    for (;;) {
      for (const log of this.peekNextLogs()) {
        this.currentLogId = log.id;
        yield log;
      }
      await new Promise(resolve => setTimeout(resolve, 100));
    }
  }

  /*
    Peek at the next batch of logs. Does not update .currentLogId.
  */
  private peekNextLogs(options: Partial<LogSearchOptions> = {}): ProcessLog[] {
    const fullOptions: LogSearchOptions = {
      ...this.options,
      ...options,
      afterLogId: this.currentLogId,
    };

    const logs = getProcessLogs(fullOptions);
    return logs;
  }

  /*
    Fetch the next batch of logs. Updates .currentLogId.
  */
  getNextLogs(options: Partial<LogSearchOptions> = {}): ProcessLog[] {
    const logs = this.peekNextLogs(options);
    if (logs.length > 0) {
      this.currentLogId = logs[logs.length - 1].id;
    }
    return logs;
  }
}
