import Path from 'path';
import os from 'os';
import { fileURLToPath } from 'url';

const __dirname = Path.dirname(fileURLToPath(import.meta.url));
export const ProjectRootDir = Path.join(__dirname, '..');


export function getStateDirectory(): string {
  // First: Use CANDLE_DATABASE_DIR if set.
  if (process.env.CANDLE_DATABASE_DIR) {
    return process.env.CANDLE_DATABASE_DIR;
  }

  // Next: Use XDG_STATE_HOME if set.
  if (process.env.XDG_STATE_HOME) {
    return Path.join(process.env.XDG_STATE_HOME, 'candle');
  }

  // Default: Use the XDG style default: ~/.local/state/candle/
  return Path.join(os.homedir(), '.local', 'state', 'candle');
}