import { existsSync, readFileSync } from 'fs';
import { dirname, join } from 'path';
import { fileURLToPath } from 'url';

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);

export function findPackageJson(): { name: string; version: string } {
  // Try '../package.json' first (for bundled dist/main-cli.js)
  const pathOne = join(__dirname, '..', 'package.json');
  if (existsSync(pathOne)) {
    return JSON.parse(readFileSync(pathOne, 'utf-8'));
  }

  // Try '../../package.json' (for source src/mcp/mcp-main.ts)
  const pathTwo = join(__dirname, '..', '..', 'package.json');
  if (existsSync(pathTwo)) {
    return JSON.parse(readFileSync(pathTwo, 'utf-8'));
  }

  throw new Error(`Could not find package.json at ${pathOne} or ${pathTwo}`);
}
