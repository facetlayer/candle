import { getDatabase } from '../database/database.ts';
import { ProcessLogType } from './ProcessLogType.ts';
import { buildLogSearchQuery } from './buildLogSearchQuery.ts';

export { ProcessLogType };

export interface NewProcessLog {
  command_name: string;
  project_dir: string;
  content?: string;
  log_type: number;
}

export interface ProcessLog {
  id: number;
  command_name: string;
  project_dir: string;
  content?: string;
  log_type: number;
  timestamp: number;
}

export interface LogSearchOptions {
  // Primary search parameters
  projectDir: string;
  commandNames: string[];

  // Filtering parameters
  limit?: number;
  sinceTimestamp?: number;
  afterLogId?: number;
  limitToLatestProcessLogs?: boolean;
}

export function saveProcessLog(processLog: NewProcessLog) {
  const db = getDatabase();
  db.run(
    'insert into process_output(command_name, project_dir, content, log_type) values(?, ?, ?, ?)',
    [processLog.command_name, processLog.project_dir, processLog.content, processLog.log_type]
  );
}


export function getProcessLogs(options: LogSearchOptions): ProcessLog[] {
  const db = getDatabase();
  const builder = buildLogSearchQuery(options);

  let logItems = db.list(builder.getSql(), builder.getParams());

  if (options.limitToLatestProcessLogs) {
    // For 'limitToLatestProcessLogs' - Stop if we find process_start_initiated
    let foundLimit = -1;
    for (let i = 0; i < logItems.length; i++) {
      const log = logItems[i];
      if (log.log_type === ProcessLogType.process_start_initiated) {
        foundLimit = i + 1;
        break;
      }
    }
    if (foundLimit !== -1) {
      logItems = logItems.slice(0, foundLimit);
    }
  }

  // Return list in chronological order
  return logItems.reverse();
}
