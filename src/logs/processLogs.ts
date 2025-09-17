import { getDatabase } from "../database/database.ts";

export const ProcessLogType = {
    stdout: 1,
    stderr: 2,

    // process_start_initiated - Saved immediately when we being launching a subprocess
    process_start_initiated: 3,

    // process_start_failed - Saved when the subprocess fails to start.
    process_start_failed: 4,

    // process_started - Saved when the subprocess has successfully started.
    process_started: 5,

    // process_exited - Saved when the subprocess exits.
    process_exited: 6,
} as const;


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
    commandName: string;
    
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
    
    const { projectDir, commandName, limit, sinceTimestamp, afterLogId } = options;

    //console.log('getProcessLogs options: ', options);
    
    // Build SQL query dynamically
    let sql: string;
    let params: any[] = [];

    // Prioritize projectDir + commandName approach
    if (projectDir !== undefined && commandName !== undefined) {
        // Primary approach: lookup by project directory and command name
        sql = `select po.* from process_output po 
               where po.project_dir = ? and po.command_name = ?`;
        params = [projectDir, commandName];
    } else if (projectDir !== undefined) {
        // Fallback: lookup by project directory only (use default command)
        sql = `select po.* from process_output po 
               where po.project_dir = ? and po.command_name = 'default'`;
        params = [projectDir];
    } else if (commandName !== undefined) {
        // Lookup by command name only (any project directory)
        sql = `select po.* from process_output po 
               where po.command_name = ?`;
        params = [commandName];
    } else {
        throw new Error('Must provide projectDir and commandName');
    }
    
    // Add timestamp filtering
    if (sinceTimestamp !== undefined) {
        sql += ' and po.timestamp > ?';
        params.push(sinceTimestamp);
    }
    
    if (afterLogId !== undefined) {
        sql += ' and po.id > ?';
        params.push(afterLogId);
    }
    
    // Order by 'desc' so that we get the most recent logs first.
    sql += ' order by po.timestamp desc, po.id desc';
    
    if (limit !== undefined) {
        sql += ' limit ?';
        params.push(limit);
    }

    //console.log('getProcessLogs sql: ', sql);
    //console.log('getProcessLogs params: ', params);

    let logItems = db.list(sql, params);

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