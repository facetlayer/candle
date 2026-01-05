import { getDatabase } from './database.ts';

export interface StdinMessage {
  id: number;
  command_name: string;
  project_dir: string;
  data: string;
  encoding: string;
  created_at: number;
}

interface CreateStdinMessage {
  commandName: string;
  projectDir: string;
  data: string;
  encoding?: string;
}

/**
 * Insert a new stdin message into the queue for a service.
 */
export function createStdinMessage(entry: CreateStdinMessage): number {
  const db = getDatabase();

  const result = db.insert('stdin_messages', {
    command_name: entry.commandName,
    project_dir: entry.projectDir,
    data: entry.data,
    encoding: entry.encoding ?? 'utf8',
  });

  return result.lastInsertRowid as number;
}

/**
 * Get and delete the next pending stdin message for a service.
 * Returns null if no messages are pending.
 */
export function popStdinMessage(
  commandName: string,
  projectDir: string
): StdinMessage | null {
  const db = getDatabase();

  // Get the oldest message for this service
  const message = db.get(
    'select * from stdin_messages where command_name = ? and project_dir = ? order by id asc limit 1',
    [commandName, projectDir]
  ) as StdinMessage | undefined;

  if (!message) {
    return null;
  }

  // Delete the message we just retrieved
  db.run('delete from stdin_messages where id = ?', [message.id]);

  return message;
}

/**
 * Delete all pending stdin messages for a service.
 */
export function clearStdinMessages(commandName: string, projectDir: string): void {
  const db = getDatabase();
  db.run('delete from stdin_messages where command_name = ? and project_dir = ?', [
    commandName,
    projectDir,
  ]);
}
