import { getDatabase } from './database.ts';

export interface ProcessEntry {
  launch_id: number;
  command_name: string;
  project_dir: string;
  pid: number;
  log_collector_pid: number;
  start_time: number;
  created_at: number;
  killed_at?: number;
  shell: string;
  root?: string;
}

interface CreateProcessEntry {
  commandName: string;
  projectDir: string;
  pid: number;
  logCollectorPid: number;
  shell: string;
  root?: string;
}

interface UpdateProcessKilledAt {
  commandName: string;
  projectDir: string;
  pid: number;
  killedAt: number;
}

interface DeleteProcessEntry {
  commandName: string;
  projectDir: string;
  pid: number;
}

export function createProcessEntry(entry: CreateProcessEntry): number {
  const db = getDatabase();

  const result = db.insert('processes', {
    command_name: entry.commandName,
    project_dir: entry.projectDir,
    pid: entry.pid,
    start_time: Math.floor(Date.now() / 1000),
    log_collector_pid: entry.logCollectorPid,
    shell: entry.shell,
    root: entry.root ?? null,
  });

  return result.lastInsertRowid as number;
}

export function updateProcessKilledAt(entry: UpdateProcessKilledAt): void {
  const db = getDatabase();
  db.run(
    'update processes set killed_at = ? where command_name = ? and project_dir = ? and pid = ?',
    [entry.killedAt, entry.commandName, entry.projectDir, entry.pid]
  );
}

export function deleteProcessEntry(entry: DeleteProcessEntry): void {
  const db = getDatabase();
  db.run('delete from processes where command_name = ? and project_dir = ? and pid = ?', [
    entry.commandName,
    entry.projectDir,
    entry.pid,
  ]);
}

export function findProcessesByCommandNameAndProjectDir(
  commandName: string,
  projectDir: string
): ProcessEntry[] {
  const db = getDatabase();
  return db.list('select * from processes where command_name = ? and project_dir = ?', [
    commandName,
    projectDir,
  ]);
}

export function findProcessesByProjectDir(projectDir: string): ProcessEntry[] {
  const db = getDatabase();
  return db.list('select * from processes where project_dir = ?', [projectDir]);
}

export function findRunningProcessesByProjectDir(projectDir: string): ProcessEntry[] {
  const db = getDatabase();
  return db.list('select * from processes where project_dir = ? and killed_at is null', [
    projectDir,
  ]);
}

export function findAllProcesses(): ProcessEntry[] {
  const db = getDatabase();
  return db.list('select * from processes');
}
