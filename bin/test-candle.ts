#!/usr/bin/env node

/**
 * Test runner for Candle CLI
 *
 * This script makes it easier to run Candle with custom environment settings
 * for testing purposes. It wraps the candle CLI and sets environment variables
 * based on the flags provided.
 *
 * Usage:
 *   bin/test-candle.ts --database-dir /tmp/test-db list
 *   bin/test-candle.ts --database-dir ./test-workspace start my-service
 *   bin/test-candle.ts --enable-logs list
 *   bin/test-candle.ts list  # Without flags, passes through normally
 */

import { runShellCommand } from '@facetlayer/subprocess-wrapper';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);

interface ParsedArgs {
  databaseDir: string | null;
  enableLogs: boolean;
  candleArgs: string[];
}

function parseArgs(argv: string[]): ParsedArgs {
  const args = argv.slice(2); // Skip node and script path
  let databaseDir: string | null = null;
  let enableLogs = false;
  const candleArgs: string[] = [];

  for (let i = 0; i < args.length; i++) {
    if (args[i] === '--database-dir') {
      if (i + 1 < args.length) {
        databaseDir = args[i + 1];
        i++; // Skip the value
      } else {
        console.error('Error: --database-dir requires a value');
        process.exit(1);
      }
    } else if (args[i] === '--enable-logs') {
      enableLogs = true;
    } else {
      candleArgs.push(args[i]);
    }
  }

  return { databaseDir, enableLogs, candleArgs };
}

async function main() {
  const { databaseDir, enableLogs, candleArgs } = parseArgs(process.argv);

  const candlePath = join(__dirname, '..', 'src', 'main-cli.ts');

  const env: Record<string, string> = { ...process.env } as Record<string, string>;

  if (databaseDir) {
    env.CANDLE_DATABASE_DIR = databaseDir;
  }

  if (enableLogs) {
    env.CANDLE_ENABLE_LOGS = 'true';
  }

  const result = await runShellCommand('node', [candlePath, ...candleArgs], {
    env,
    cwd: process.cwd(),
  });

  // Print stdout and stderr (they are arrays of lines)
  for (const line of result.stdout ?? []) {
    console.log(line);
  }
  for (const line of result.stderr ?? []) {
    console.error(line);
  }

  process.exit(result.exitCode ?? 0);
}

main().catch(error => {
  console.error(error);
  process.exit(1);
});
