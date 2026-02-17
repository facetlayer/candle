import { useState, useEffect, useRef, useCallback } from 'react';
import { webFetch } from '@facetlayer/prism-framework-ui';
import { usePolling } from '../hooks/usePolling';

const LogType = {
  stdout: 1,
  stderr: 2,
  process_start_initiated: 3,
  process_start_failed: 4,
  process_started: 5,
  process_exited: 6,
} as const;

interface LogEntry {
  id: number;
  command_name: string;
  content?: string;
  log_type: number;
  timestamp: number;
}

interface LogViewerProps {
  serviceName: string | null;
  projectDir: string | null;
}

function formatTimestamp(unix: number): string {
  const date = new Date(unix * 1000);
  return date.toLocaleTimeString('en-US', { hour12: false, hour: '2-digit', minute: '2-digit', second: '2-digit' });
}

function getLogClass(logType: number): string {
  switch (logType) {
    case LogType.stdout: return 'stdout';
    case LogType.stderr: return 'stderr';
    default: return 'lifecycle';
  }
}

function getLifecycleLabel(logType: number): string {
  switch (logType) {
    case LogType.process_start_initiated: return '[Starting...]';
    case LogType.process_start_failed: return '[Start failed]';
    case LogType.process_started: return '[Process started]';
    case LogType.process_exited: return '[Process exited]';
    default: return '';
  }
}

function serviceKey(name: string | null, dir: string | null) {
  return `${dir}:${name}`;
}

export function LogViewer({ serviceName, projectDir }: LogViewerProps) {
  const [logs, setLogs] = useState<LogEntry[]>([]);
  const [autoScroll, setAutoScroll] = useState(true);
  const logContainerRef = useRef<HTMLDivElement>(null);
  const lastLogIdRef = useRef<number>(0);
  const currentKeyRef = useRef<string>('');

  useEffect(() => {
    const key = serviceKey(serviceName, projectDir);
    if (key !== currentKeyRef.current) {
      setLogs([]);
      lastLogIdRef.current = 0;
      currentKeyRef.current = key;
    }
  }, [serviceName, projectDir]);

  const fetchLogs = useCallback(async () => {
    if (!serviceName || !projectDir) return;

    try {
      const params: Record<string, any> = { name: serviceName, projectDir };
      if (lastLogIdRef.current > 0) {
        params.afterLogId = lastLogIdRef.current;
      } else {
        params.limit = 200;
      }

      const data = await webFetch('GET /services/:name/logs', { params });
      const newLogs: LogEntry[] = data.logs || [];

      if (newLogs.length > 0) {
        lastLogIdRef.current = newLogs[newLogs.length - 1].id;
        setLogs(prev => {
          const combined = [...prev, ...newLogs];
          if (combined.length > 2000) {
            return combined.slice(-2000);
          }
          return combined;
        });
      }
    } catch {
      // Silently ignore fetch errors during polling
    }
  }, [serviceName, projectDir]);

  usePolling(fetchLogs, 1000);

  useEffect(() => {
    if (autoScroll && logContainerRef.current) {
      logContainerRef.current.scrollTop = logContainerRef.current.scrollHeight;
    }
  }, [logs, autoScroll]);

  if (!serviceName) {
    return (
      <div className="log-viewer">
        <div className="log-placeholder">Select a service to view logs</div>
      </div>
    );
  }

  return (
    <div className="log-viewer">
      <div className="log-viewer-header">
        <h2>
          Logs <span className="service-label">{serviceName}</span>
        </h2>
        <button
          className={`auto-scroll-toggle ${autoScroll ? 'active' : ''}`}
          onClick={() => setAutoScroll(!autoScroll)}
        >
          Auto-scroll: {autoScroll ? 'On' : 'Off'}
        </button>
      </div>
      <div className="log-content" ref={logContainerRef}>
        {logs.length === 0 && (
          <div className="log-entry lifecycle">No logs available</div>
        )}
        {logs.map(log => {
          const logClass = getLogClass(log.log_type);
          const isLifecycle = log.log_type >= LogType.process_start_initiated;
          const content = isLifecycle
            ? (log.content || getLifecycleLabel(log.log_type))
            : (log.content || '');

          return (
            <div key={log.id} className={`log-entry ${logClass}`}>
              <span className="log-timestamp">{formatTimestamp(log.timestamp)}</span>
              {content}
            </div>
          );
        })}
      </div>
    </div>
  );
}
