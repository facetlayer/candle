import { getDatabase } from './database/database.ts';

interface ClearLogsCommandOptions {
  projectDir: string;
  commandNames: string[];
}

export async function handleClearLogsCommand(options: ClearLogsCommandOptions): Promise<void> {
  const { projectDir, commandNames } = options;
  const db = getDatabase();

  console.log(`Clearing logs for project: ${projectDir}`);

  try {
    let clearedCount = 0;

    for (const commandName of commandNames) {
      // Clear logs for this specific project directory and command
      const result = db.run(
        `
              DELETE FROM process_output 
              WHERE command_name = ? AND project_dir = ?
          `,
        [commandName, projectDir]
      );
      clearedCount += result.changes || 0;
    }

    if (clearedCount > 0) {
      console.log(`✓ Cleared ${clearedCount} log entries`);
    } else {
      console.log('- No logs found to clear');
    }

    // Clean up orphaned logs and optimize database
    db.run(
      `DELETE FROM process_output WHERE (command_name, project_dir) NOT IN (SELECT command_name, project_dir FROM processes)`
    );
    db.run('VACUUM');

    console.log('\nLogs cleared successfully!');
  } catch (error) {
    console.error('Error clearing logs:', error);
    process.exit(1);
  }
}
