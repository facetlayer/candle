import { describe, it, expect } from 'vitest';
import { LatestExecutionLogFilter } from '../LatestExecutionLogFilter.ts';
import { ProcessLogType } from '../../logs/ProcessLogType.ts';
import { type ProcessLog } from '../../logs/processLogs.ts';

// Timestamps in the database are stored in *seconds* (the column default is
// strftime('%s', 'now')), so test logs use second-resolution timestamps to match
// what the filter actually receives at runtime.
const NOW_SECONDS = Math.floor(Date.now() / 1000);

let nextId = 1;

function makeLog(overrides: Partial<ProcessLog> = {}): ProcessLog {
  return {
    id: nextId++,
    command_name: 'svc',
    project_dir: '/project',
    content: 'hello',
    log_type: ProcessLogType.stdout,
    timestamp: NOW_SECONDS,
    ...overrides,
  };
}

describe('LatestExecutionLogFilter', () => {
  it('shows all logs from previous launch when no recency window is set', () => {
    const filter = new LatestExecutionLogFilter({
      showPastLogsBehavior: 'show_logs_from_previous_launch',
    });
    const logs = [makeLog({ content: 'a' }), makeLog({ content: 'b' })];
    filter.checkLatestLaunchStatus(logs);

    const result = filter.filter(logs);
    expect(result.map(l => l.content)).toEqual(['a', 'b']);
  });

  it('shows recent logs (within the window) when a recency window is set', () => {
    // This is the regression test for the `candle watch` bug: with second-resolution
    // timestamps and a millisecond window, the window comparison must use matching units.
    const filter = new LatestExecutionLogFilter({
      showPastLogsBehavior: 'show_logs_from_previous_launch',
      recentWindowMs: 10_000,
    });
    const logs = [
      makeLog({ content: 'recent-1', timestamp: NOW_SECONDS }),
      makeLog({ content: 'recent-2', timestamp: NOW_SECONDS - 2 }),
    ];
    filter.checkLatestLaunchStatus(logs);

    const result = filter.filter(logs);
    expect(result.map(l => l.content)).toEqual(['recent-1', 'recent-2']);
  });

  it('hides logs older than the recency window', () => {
    const filter = new LatestExecutionLogFilter({
      showPastLogsBehavior: 'show_logs_from_previous_launch',
      recentWindowMs: 10_000,
    });
    const logs = [
      makeLog({ content: 'old', timestamp: NOW_SECONDS - 60 }),
      makeLog({ content: 'recent', timestamp: NOW_SECONDS }),
    ];
    filter.checkLatestLaunchStatus(logs);

    const result = filter.filter(logs);
    expect(result.map(l => l.content)).toEqual(['recent']);
  });

  it('only shows logs from the most recent launch', () => {
    const filter = new LatestExecutionLogFilter({
      showPastLogsBehavior: 'show_logs_from_previous_launch',
    });
    const logs = [
      makeLog({ content: 'old-run', log_type: ProcessLogType.stdout, timestamp: NOW_SECONDS - 5 }),
      makeLog({ content: 'relaunch', log_type: ProcessLogType.process_start_initiated, timestamp: NOW_SECONDS - 1 }),
      makeLog({ content: 'new-run', log_type: ProcessLogType.stdout, timestamp: NOW_SECONDS }),
    ];
    filter.checkLatestLaunchStatus(logs);

    const result = filter.filter(logs);
    expect(result.map(l => l.content)).toEqual(['relaunch', 'new-run']);
  });
});
