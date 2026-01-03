import { describe, it, expect } from 'vitest';
import { AfterProcessStartLogFilter } from '../afterProcessStartFilter.ts';
import { ProcessLogType, type ProcessLog } from '../logs/processLogs.ts';

function makeLog(commandName: string, logType: number, content: string = ''): ProcessLog {
  return {
    id: Math.floor(Math.random() * 10000),
    command_name: commandName,
    project_dir: '/test',
    content,
    log_type: logType,
    timestamp: Date.now(),
  };
}

describe('AfterProcessStartLogFilter', () => {
  it('should filter out logs before process_start_initiated', () => {
    const filter = new AfterProcessStartLogFilter();

    const logs: ProcessLog[] = [
      makeLog('service-a', ProcessLogType.stdout, 'old log 1'),
      makeLog('service-a', ProcessLogType.stdout, 'old log 2'),
      makeLog('service-a', ProcessLogType.process_start_initiated, 'starting'),
      makeLog('service-a', ProcessLogType.stdout, 'new log 1'),
      makeLog('service-a', ProcessLogType.stdout, 'new log 2'),
    ];

    const result = filter.filter(logs);

    expect(result).toHaveLength(3);
    expect(result[0].content).toBe('starting');
    expect(result[1].content).toBe('new log 1');
    expect(result[2].content).toBe('new log 2');
  });

  it('should handle multiple commands independently', () => {
    const filter = new AfterProcessStartLogFilter();

    const logs: ProcessLog[] = [
      makeLog('service-a', ProcessLogType.stdout, 'a-old'),
      makeLog('service-b', ProcessLogType.stdout, 'b-old'),
      makeLog('service-a', ProcessLogType.process_start_initiated, 'a-start'),
      makeLog('service-a', ProcessLogType.stdout, 'a-new'),
      makeLog('service-b', ProcessLogType.stdout, 'b-still-old'),
      makeLog('service-b', ProcessLogType.process_start_initiated, 'b-start'),
      makeLog('service-b', ProcessLogType.stdout, 'b-new'),
    ];

    const result = filter.filter(logs);

    expect(result).toHaveLength(4);
    expect(result.map(l => l.content)).toEqual([
      'a-start',
      'a-new',
      'b-start',
      'b-new',
    ]);
  });

  it('should include all logs after start for a command', () => {
    const filter = new AfterProcessStartLogFilter();

    const logs: ProcessLog[] = [
      makeLog('service-a', ProcessLogType.process_start_initiated, 'start'),
      makeLog('service-a', ProcessLogType.stdout, 'stdout 1'),
      makeLog('service-a', ProcessLogType.stderr, 'stderr 1'),
      makeLog('service-a', ProcessLogType.process_exited, 'exited'),
    ];

    const result = filter.filter(logs);

    expect(result).toHaveLength(4);
  });

  it('should filter all logs if no process_start_initiated seen', () => {
    const filter = new AfterProcessStartLogFilter();

    const logs: ProcessLog[] = [
      makeLog('service-a', ProcessLogType.stdout, 'log 1'),
      makeLog('service-a', ProcessLogType.stdout, 'log 2'),
      makeLog('service-a', ProcessLogType.stderr, 'error'),
    ];

    const result = filter.filter(logs);

    expect(result).toHaveLength(0);
  });

  it('should maintain state across multiple filter calls', () => {
    const filter = new AfterProcessStartLogFilter();

    // First batch: see the start
    const batch1: ProcessLog[] = [
      makeLog('service-a', ProcessLogType.stdout, 'old'),
      makeLog('service-a', ProcessLogType.process_start_initiated, 'start'),
    ];

    const result1 = filter.filter(batch1);
    expect(result1).toHaveLength(1);
    expect(result1[0].content).toBe('start');

    // Second batch: should include logs since we saw start
    const batch2: ProcessLog[] = [
      makeLog('service-a', ProcessLogType.stdout, 'new log'),
    ];

    const result2 = filter.filter(batch2);
    expect(result2).toHaveLength(1);
    expect(result2[0].content).toBe('new log');
  });

  it('should return empty array for empty input', () => {
    const filter = new AfterProcessStartLogFilter();
    const result = filter.filter([]);
    expect(result).toHaveLength(0);
  });
});
