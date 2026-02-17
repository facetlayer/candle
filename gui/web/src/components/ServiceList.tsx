import { useState, useCallback } from 'react';
import { webFetch } from '@facetlayer/prism-framework-ui';
import type { SelectedService } from '../App';
import { usePolling } from '../hooks/usePolling';

interface ServiceProcess {
  serviceName: string;
  projectDir: string;
  pid: number;
  uptime: string;
  status: string;
  shell: string;
  root: string | null;
}

interface ListResponse {
  processes: ServiceProcess[];
}

interface ProjectGroup {
  projectDir: string;
  displayName: string;
  services: ServiceProcess[];
}

interface ServiceListProps {
  selected: SelectedService | null;
  onSelect: (service: SelectedService | null) => void;
}

function getDisplayName(projectDir: string): string {
  const parts = projectDir.split('/');
  // Show last 2 path segments for context, e.g. "andy/candle"
  return parts.slice(-2).join('/');
}

function groupByProject(processes: ServiceProcess[]): ProjectGroup[] {
  const map = new Map<string, ServiceProcess[]>();
  for (const proc of processes) {
    const existing = map.get(proc.projectDir);
    if (existing) {
      existing.push(proc);
    } else {
      map.set(proc.projectDir, [proc]);
    }
  }

  return Array.from(map.entries())
    .sort(([a], [b]) => a.localeCompare(b))
    .map(([projectDir, services]) => ({
      projectDir,
      displayName: getDisplayName(projectDir),
      services,
    }));
}

export function ServiceList({ selected, onSelect }: ServiceListProps) {
  const [services, setServices] = useState<ServiceProcess[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [actionInProgress, setActionInProgress] = useState<string | null>(null);

  const fetchServices = useCallback(async () => {
    try {
      const data: ListResponse = await webFetch('/services');
      setServices(data.processes);
      setError(null);
    } catch {
      setError('Failed to fetch services');
    } finally {
      setLoading(false);
    }
  }, []);

  usePolling(fetchServices, 2000);

  const handleAction = async (serviceName: string, projectDir: string, action: 'start' | 'restart' | 'kill') => {
    const key = `${projectDir}:${serviceName}:${action}`;
    setActionInProgress(key);
    try {
      await webFetch(`POST /services/:name/${action}`, {
        params: { name: serviceName, projectDir },
      });
      await fetchServices();
    } catch {
      setError(`Failed to ${action} '${serviceName}'`);
    } finally {
      setActionInProgress(null);
    }
  };

  const handleOpenBrowser = async (serviceName: string, projectDir: string) => {
    const key = `${projectDir}:${serviceName}:open`;
    setActionInProgress(key);
    try {
      const data = await webFetch('GET /services/:name/url', {
        params: { name: serviceName, projectDir },
      });
      if (data.url) {
        window.open(data.url, '_blank');
      } else {
        setError(`No open port found for '${serviceName}'`);
      }
    } catch {
      setError(`Failed to get URL for '${serviceName}'`);
    } finally {
      setActionInProgress(null);
    }
  };

  const isSelected = (svc: ServiceProcess) =>
    selected?.serviceName === svc.serviceName && selected?.projectDir === svc.projectDir;

  const groups = groupByProject(services);

  if (loading) {
    return (
      <div className="service-list">
        <div className="loading">Loading services...</div>
      </div>
    );
  }

  return (
    <div className="service-list">
      <div className="service-list-header">
        <h2>Services</h2>
        <button className="refresh-btn" onClick={fetchServices}>Refresh</button>
      </div>
      {error && <div className="error-message">{error}</div>}
      <div className="service-items">
        {groups.length === 0 && (
          <div className="loading">No running services</div>
        )}
        {groups.map(group => (
          <div key={group.projectDir} className="project-group">
            <div className="project-header" title={group.projectDir}>
              <span className="project-icon">&#x25B8;</span>
              <span className="project-name">{group.displayName}</span>
              <span className="project-count">{group.services.length}</span>
            </div>
            {group.services.map(service => {
              const isRunning = service.status === 'RUNNING';
              const actionKey = `${service.projectDir}:${service.serviceName}:`;

              return (
                <div
                  key={`${service.projectDir}:${service.serviceName}:${service.pid}`}
                  className={`service-item ${isSelected(service) ? 'selected' : ''}`}
                  onClick={() => onSelect({ serviceName: service.serviceName, projectDir: service.projectDir })}
                >
                  <div className="service-item-header">
                    <span className="service-name">{service.serviceName}</span>
                    <span className={`service-status ${isRunning ? 'running' : 'stopped'}`}>
                      {isRunning ? 'Running' : 'Stopped'}
                    </span>
                  </div>
                  <div className="service-meta">
                    {isRunning && <span>PID: {service.pid}</span>}
                    {isRunning && <span>Uptime: {service.uptime}</span>}
                  </div>
                  <div className="service-actions">
                    {isRunning && (
                      <>
                        <button
                          className="action-btn open"
                          disabled={actionInProgress === `${actionKey}open`}
                          onClick={(e) => { e.stopPropagation(); handleOpenBrowser(service.serviceName, service.projectDir); }}
                        >
                          {actionInProgress === `${actionKey}open` ? 'Opening...' : 'Open'}
                        </button>
                        <button
                          className="action-btn restart"
                          disabled={actionInProgress === `${actionKey}restart`}
                          onClick={(e) => { e.stopPropagation(); handleAction(service.serviceName, service.projectDir, 'restart'); }}
                        >
                          {actionInProgress === `${actionKey}restart` ? 'Restarting...' : 'Restart'}
                        </button>
                        <button
                          className="action-btn kill"
                          disabled={actionInProgress === `${actionKey}kill`}
                          onClick={(e) => { e.stopPropagation(); handleAction(service.serviceName, service.projectDir, 'kill'); }}
                        >
                          {actionInProgress === `${actionKey}kill` ? 'Killing...' : 'Kill'}
                        </button>
                      </>
                    )}
                  </div>
                </div>
              );
            })}
          </div>
        ))}
      </div>
    </div>
  );
}
