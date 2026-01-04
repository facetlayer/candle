import {
  deleteProcessEntry,
  updateProcessKilledAt,
} from '../database/processTable.ts';
import { killProcessTree } from './killProcessTree.ts';

export interface KillProcessOptions {
  quiet?: boolean; // If enabled, don't print any output
}

export interface ServiceInfo {
  command_name: string;
  project_dir: string;
  pid: number | null;
  killed_at?: number | null;
}

export async function killOneRunningProcess(process: ServiceInfo, options: KillProcessOptions) {
  // Kill the process and its entire process tree
  if (process.pid) {
    const result = await killProcessTree(process.pid);
    if (result === 'success') {
      if (!options.quiet) {
        console.log(`[Killed '${process.command_name}' process with PID: ${process.pid}]`);
      }

      // If the killed_at date is over 5 minutes old, delete the stale entry.
      // This can happen if the entry fails to get cleaned up.

      if (process.killed_at && process.killed_at < Date.now() - 5 * 60 * 1000) {
        if (!options.quiet) {
          console.warn(
            `[Cleaning up stale process entry for '${process.command_name}' with PID: ${process.pid}]`
          );
        }
        deleteProcessEntry({
          commandName: process.command_name,
          projectDir: process.project_dir,
          pid: process.pid,
        });
      } else {
        updateProcessKilledAt({
          commandName: process.command_name,
          projectDir: process.project_dir,
          pid: process.pid,
          killedAt: Math.floor(Date.now() / 1000),
        });
      }
    } else if (result === 'process_not_found') {
      if (!options.quiet) {
        console.warn(
          `[Cleaning up stale process entry for '${process.command_name}' with PID: ${process.pid}]`
        );
      }
      deleteProcessEntry({
        commandName: process.command_name,
        projectDir: process.project_dir,
        pid: process.pid,
      });
    } else {
      if (!options.quiet) {
        console.log(`Error killing process '${process.command_name}' with PID: ${process.pid}`);
      }
    }
  }
}
