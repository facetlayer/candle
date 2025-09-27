import * as fs from 'fs';
import * as os from 'os';
import * as path from 'path';

export async function handleClearDatabaseCommand(): Promise<void> {
  const stateDir = process.env.LOCAL_SERVER_STATE_DIR || path.join(os.homedir(), '.candle');
  const dbPath = path.join(stateDir, 'candle.db');
  const walPath = path.join(stateDir, 'candle.db-wal');
  const shmPath = path.join(stateDir, 'candle.db-shm');

  console.log(`Clearing database at: ${dbPath}`);

  try {
    // Remove the main database file
    if (fs.existsSync(dbPath)) {
      fs.unlinkSync(dbPath);
      console.log('✓ Removed database file');
    } else {
      console.log('- Database file not found');
    }

    // Remove WAL file if it exists
    if (fs.existsSync(walPath)) {
      fs.unlinkSync(walPath);
      console.log('✓ Removed WAL file');
    }

    // Remove shared memory file if it exists
    if (fs.existsSync(shmPath)) {
      fs.unlinkSync(shmPath);
      console.log('✓ Removed shared memory file');
    }

    console.log('\nDatabase cleared successfully!');
    console.log('A new database will be created on next use.');
  } catch (error) {
    console.error('Error clearing database:', error);
    process.exit(1);
  }
}
