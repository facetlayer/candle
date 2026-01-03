import { getDatabase } from '../database/database.ts';
import { buildLogSearchQuery } from './buildLogSearchQuery.ts';

const VerboseLogs = false;

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

  if (VerboseLogs) {
    console.log('getProcessLogs - running SQL', builder.getSql(), builder.getParams());
  }
  
  const logItems = db.list(builder.getSql(), builder.getParams());

  // Return list in chronological order
  const sorted = logItems.reverse();

  if (VerboseLogs) {
    console.log('getProcessLogs - got logs', sorted);
  }

  return sorted;
}
