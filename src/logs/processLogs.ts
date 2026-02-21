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
  commandNames?: string[]; // If empty/undefined, returns logs for all commands in the project

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


export interface ProcessLogResult {
  logs: ProcessLog[];
  logsWereEvicted: boolean;
}

export function getProcessLogs(options: LogSearchOptions): ProcessLog[] {
  return getProcessLogsWithEvictionInfo(options).logs;
}

export function getProcessLogsWithEvictionInfo(options: LogSearchOptions): ProcessLogResult {
  const db = getDatabase();
  const builder = buildLogSearchQuery(options);

  if (VerboseLogs) {
    console.log('getProcessLogs - running SQL', builder.getSql(), builder.getParams());
  }

  const logItems = db.list(builder.getSql(), builder.getParams());

  // Check if there are more logs beyond our limit (indicating eviction/truncation)
  let logsWereEvicted = false;
  if (options.limit !== undefined && logItems.length >= options.limit) {
    // We got exactly the limit - there may be more logs we didn't fetch
    const countBuilder = buildLogSearchQuery({ ...options, limit: undefined });
    const countSql = `select count(*) as total from (${countBuilder.getSql()})`;
    const countResult = db.get(countSql, countBuilder.getParams());
    if (countResult && countResult.total > logItems.length) {
      logsWereEvicted = true;
    }
  }

  // Return list in chronological order
  const sorted = logItems.reverse();

  if (VerboseLogs) {
    console.log('getProcessLogs - got logs', sorted);
  }

  return { logs: sorted, logsWereEvicted };
}
