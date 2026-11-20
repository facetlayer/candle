import { useCallback, useEffect, useMemo, useState } from 'react';
import { createRoot } from 'react-dom/client';

const REFRESH_MS = 2000;

async function fetchProcesses() {
    const res = await fetch('/api/processes');
    if (!res.ok) throw new Error(`Server returned ${res.status}`);
    return res.json();
}

async function openInBrowser(proc, port) {
    const res = await fetch('/api/open-browser', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ projectDir: proc.projectDir, serviceName: proc.serviceName, port }),
    });
    const body = await res.json().catch(() => ({}));
    if (!res.ok) throw new Error(body.error || `Server returned ${res.status}`);
    return body;
}

/** Poll the process list. Keeps the last good data while a refresh fails. */
function useProcesses() {
    const [data, setData] = useState(null);
    const [error, setError] = useState(null);
    const [updatedAt, setUpdatedAt] = useState(null);

    const refresh = useCallback(async () => {
        try {
            setData(await fetchProcesses());
            setError(null);
            setUpdatedAt(new Date());
        } catch (e) {
            setError(e.message || String(e));
        }
    }, []);

    useEffect(() => {
        refresh();
        const id = setInterval(refresh, REFRESH_MS);
        return () => clearInterval(id);
    }, [refresh]);

    return { data, error, updatedAt, refresh };
}

function groupByProject(processes) {
    const groups = new Map();
    for (const p of processes) {
        if (!groups.has(p.projectDir)) groups.set(p.projectDir, []);
        groups.get(p.projectDir).push(p);
    }
    // The global namespace (the dashboard itself) sorts last.
    return [...groups.entries()]
        .map(([projectDir, procs]) => ({ projectDir, isGlobal: procs[0].isGlobal, procs }))
        .sort((a, b) => a.isGlobal - b.isGlobal || a.projectDir.localeCompare(b.projectDir));
}

function basename(dir) {
    const parts = dir.split('/').filter(Boolean);
    return parts[parts.length - 1] || dir;
}

function matches(proc, query) {
    if (!query) return true;
    const q = query.toLowerCase();
    return [proc.serviceName, proc.projectDir, proc.command, String(proc.pid ?? '')]
        .concat(proc.ports.map((p) => String(p.port)))
        .some((field) => field.toLowerCase().includes(q));
}

function PortButton({ proc, port, onResult }) {
    const [busy, setBusy] = useState(false);
    const click = async () => {
        setBusy(true);
        try {
            const out = await openInBrowser(proc, port.port);
            onResult({ ok: true, text: `Opened ${out.url}` });
        } catch (e) {
            onResult({ ok: false, text: e.message });
        } finally {
            setBusy(false);
        }
    };
    return (
        <button
            className="port-button"
            onClick={click}
            disabled={busy}
            title={`Open http://localhost:${port.port} (${port.address}${port.isChildProcess ? ', child process' : ''})`}
        >
            Open :{port.port}
        </button>
    );
}

function ServiceRow({ proc, onResult }) {
    return (
        <tr>
            <td>
                <div className="name-cell">
                    <span className="status-dot" aria-hidden="true" />
                    <span className="service-name">{proc.serviceName}</span>
                    {proc.transient && <span className="tag" title="Started with --shell">transient</span>}
                </div>
            </td>
            <td className="mono num">{proc.pid}</td>
            <td className="num">{proc.uptime}</td>
            <td className="command-cell">
                <code title={`${proc.command}\n\nin ${proc.workingDir}`}>{proc.command}</code>
            </td>
            <td>
                <div className="ports-cell">
                    {proc.ports.length === 0 ? (
                        <span className="muted">No ports</span>
                    ) : (
                        proc.ports.map((port) => (
                            <PortButton key={port.port} proc={proc} port={port} onResult={onResult} />
                        ))
                    )}
                </div>
            </td>
        </tr>
    );
}

function ProjectCard({ group, onResult }) {
    const title = group.isGlobal ? 'Candle (global)' : basename(group.projectDir);
    return (
        <section className="project">
            <header className="project-header">
                <h2>{title}</h2>
                <span className="project-dir mono" title={group.projectDir}>
                    {group.projectDir}
                </span>
                <span className="count">
                    {group.procs.length} {group.procs.length === 1 ? 'service' : 'services'}
                </span>
            </header>
            <table>
                <thead>
                    <tr>
                        <th>Service</th>
                        <th className="num">PID</th>
                        <th className="num">Uptime</th>
                        <th>Command</th>
                        <th>Ports</th>
                    </tr>
                </thead>
                <tbody>
                    {group.procs.map((proc) => (
                        <ServiceRow key={`${proc.serviceName}:${proc.pid}`} proc={proc} onResult={onResult} />
                    ))}
                </tbody>
            </table>
        </section>
    );
}

function Toast({ message, onDone }) {
    useEffect(() => {
        if (!message) return;
        const id = setTimeout(onDone, 4000);
        return () => clearTimeout(id);
    }, [message, onDone]);
    if (!message) return null;
    return (
        <div className={`toast ${message.ok ? 'ok' : 'error'}`} role="status">
            {message.text}
        </div>
    );
}

function App() {
    const { data, error, updatedAt } = useProcesses();
    const [query, setQuery] = useState('');
    const [toast, setToast] = useState(null);
    const clearToast = useCallback(() => setToast(null), []);

    const processes = data?.processes ?? [];
    const groups = useMemo(
        () => groupByProject(processes.filter((p) => matches(p, query))),
        [processes, query],
    );
    const projectCount = new Set(processes.map((p) => p.projectDir)).size;

    return (
        <div className="page">
            <header className="top">
                <div className="brand">
                    <h1>Candle</h1>
                    {data && <span className="version">v{data.version}</span>}
                </div>
                <p className="summary">
                    {data
                        ? `${processes.length} running ${processes.length === 1 ? 'service' : 'services'} across ${projectCount} ${projectCount === 1 ? 'project' : 'projects'}`
                        : 'Loading…'}
                </p>
                <input
                    className="filter"
                    type="search"
                    placeholder="Filter by name, project, command, or port"
                    value={query}
                    onChange={(e) => setQuery(e.target.value)}
                    aria-label="Filter services"
                />
            </header>

            {error && (
                <div className="banner error">
                    Can't reach the Candle dashboard server ({error}). Showing the last known state.
                </div>
            )}

            {data && processes.length === 0 && (
                <div className="empty">
                    <p>No services are running.</p>
                    <p className="muted">
                        Start one with <code>candle start</code> in a project directory.
                    </p>
                </div>
            )}

            {data && processes.length > 0 && groups.length === 0 && (
                <div className="empty muted">No services match “{query}”.</div>
            )}

            {groups.map((group) => (
                <ProjectCard key={group.projectDir} group={group} onResult={setToast} />
            ))}

            <footer className="footer muted">
                {updatedAt ? `Updated ${updatedAt.toLocaleTimeString()}` : ''} · refreshes every{' '}
                {REFRESH_MS / 1000}s
            </footer>

            <Toast message={toast} onDone={clearToast} />
        </div>
    );
}

createRoot(document.getElementById('root')).render(<App />);
