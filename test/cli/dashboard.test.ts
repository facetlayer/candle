import * as http from 'http';
import { afterAll, beforeAll, describe, expect, it } from 'vitest';
import { TestWorkspace } from './utils';

const workspace = new TestWorkspace('cli-dashboard');

interface HttpResult {
  status: number;
  contentType: string;
  body: string;
}

/** Plain node:http so the test can set the Host header (fetch forbids it). */
function request(
  port: number,
  path: string,
  options: { method?: string; headers?: Record<string, string>; body?: string } = {}
): Promise<HttpResult> {
  return new Promise((resolve, reject) => {
    const req = http.request(
      {
        host: '127.0.0.1',
        port,
        path,
        method: options.method ?? 'GET',
        // Like a browser's fetch, send a Content-Length rather than a chunked body.
        headers: { ...options.headers, 'Content-Length': Buffer.byteLength(options.body ?? '') },
      },
      res => {
        let body = '';
        res.setEncoding('utf8');
        res.on('data', chunk => (body += chunk));
        res.on('end', () =>
          resolve({
            status: res.statusCode!,
            contentType: String(res.headers['content-type']),
            body,
          })
        );
      }
    );
    req.on('error', reject);
    if (options.body) req.write(options.body);
    req.end();
  });
}

describe('CLI dashboard open / close / run', () => {
  let port: number;

  beforeAll(async () => {
    await workspace.runCli(['start', 'web']);
    await workspace.runCli(['wait-for-log', 'web', '--message', 'Now listening']);
    const result = await workspace.runCli(['dashboard', 'open', '--no-browser']);
    const match = result
      .stdoutAsString()
      .match(/Started the Candle dashboard at http:\/\/localhost:(\d+)/);
    expect(match).not.toBeNull();
    port = Number(match![1]);
  });

  afterAll(() => workspace.cleanup());

  it('reuses the running dashboard on a second call', async () => {
    const result = await workspace.runCli(['dashboard', 'open', '--no-browser']);
    expect(result.stdoutAsString()).toContain(
      `The Candle dashboard is running at http://localhost:${port}`
    );
    expect(result.stdoutAsString()).not.toContain('Opened');
  });

  it('serves the bundled UI', async () => {
    const index = await request(port, '/');
    expect(index.status).toBe(200);
    expect(index.contentType).toContain('text/html');
    expect(index.body).toContain('<script src="/app.js">');

    const js = await request(port, '/app.js');
    expect(js.status).toBe(200);
    expect(js.contentType).toContain('javascript');
    expect(js.body.length).toBeGreaterThan(1000);
  });

  it('lists running services from every project with their ports', async () => {
    const res = await request(port, '/api/processes');
    expect(res.status).toBe(200);
    const data = JSON.parse(res.body);

    const web = data.processes.find((p: any) => p.serviceName === 'web');
    expect(web).toBeDefined();
    expect(web.projectDir).toBe(workspace.dbDir);
    expect(web.isGlobal).toBe(false);
    expect(web.ports.length).toBeGreaterThan(0);

    const dashboard = data.processes.find((p: any) => p.serviceName === 'dashboard');
    expect(dashboard).toBeDefined();
    expect(dashboard.isGlobal).toBe(true);
    expect(dashboard.projectDir).toBe(data.globalProjectDir);
    expect(dashboard.ports.map((p: any) => p.port)).toContain(port);
  });

  it('rejects requests addressed to a non-loopback host', async () => {
    const res = await request(port, '/api/processes', { headers: { Host: 'attacker.example' } });
    expect(res.status).toBe(403);
  });

  it('open-browser only opens ports the service is listening on', async () => {
    const json = { 'Content-Type': 'application/json' };
    const res = await request(port, '/api/open-browser', {
      method: 'POST',
      headers: json,
      body: JSON.stringify({ projectDir: workspace.dbDir, serviceName: 'web', port: 1 }),
    });
    expect(res.status).toBe(400);
    expect(JSON.parse(res.body).error).toContain('not listening on port 1');

    const noJson = await request(port, '/api/open-browser', { method: 'POST', body: '{}' });
    expect(noJson.status).toBe(415);
  });

  it('keeps the dashboard out of project listings and orphan reports', async () => {
    const list = await workspace.runCli(['list', '--json']);
    expect(JSON.parse(list.stdoutAsString()).map((p: any) => p.serviceName)).toEqual(['web']);

    const orphans = await workspace.runCli(['find-orphans']);
    expect(orphans.stdoutAsString()).toContain('No orphaned processes found');

    const all = await workspace.runCli(['list-all']);
    expect(all.stdoutAsString()).toMatch(/^dashboard\s+RUNNING/m);
  });

  it('rejects an out-of-range --port', async () => {
    const result = await workspace.runCli(['dashboard', 'open', '--port', '70000'], {
      ignoreExitCode: true,
    });
    expect(result.failed()).toBe(true);
    expect(result.stderrAsString()).toContain('--port must be between 1 and 65535');
  });

  it('rejects a missing or unknown subcommand and misplaced flags', async () => {
    const cases: [string[], string][] = [
      [['dashboard'], 'dashboard requires a subcommand: open, close, or run'],
      [['dashboard', 'bogus'], "Unknown dashboard subcommand 'bogus'"],
      [['dashboard', 'close', '--port', '8000'], "--port does not apply to 'dashboard close'"],
      [['dashboard', 'run', '--no-browser'], "--no-browser does not apply to 'dashboard run'"],
    ];
    for (const [args, message] of cases) {
      const result = await workspace.runCli(args, { ignoreExitCode: true });
      expect(result.failed()).toBe(true);
      expect(result.stderrAsString()).toContain(message);
    }
  });

  it('kill stops a service, refusing stale pids and the dashboard itself', async () => {
    const kill = (body: object) =>
      request(port, '/api/kill', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(body),
      });
    const processes = JSON.parse((await request(port, '/api/processes')).body);
    const web = processes.processes.find((p: any) => p.serviceName === 'web');
    const dashboard = processes.processes.find((p: any) => p.serviceName === 'dashboard');

    const own = await kill({ projectDir: dashboard.projectDir, serviceName: 'dashboard' });
    expect(own.status).toBe(400);
    expect(JSON.parse(own.body).error).toContain('candle dashboard close');

    const stale = await kill({ projectDir: web.projectDir, serviceName: 'web', pid: web.pid + 1 });
    expect(stale.status).toBe(409);

    const killed = await kill({ projectDir: web.projectDir, serviceName: 'web', pid: web.pid });
    expect(killed.status).toBe(200);
    expect(JSON.parse(killed.body)).toEqual({ serviceName: 'web', pid: web.pid });

    const list = await workspace.runCli(['list', '--json']);
    expect(JSON.parse(list.stdoutAsString())[0].status).toBe('not running');

    const again = await kill({ projectDir: web.projectDir, serviceName: 'web' });
    expect(again.status).toBe(404);
  });

  // Runs last: stops the dashboard the other tests share.
  it('close stops the dashboard, and says so when it is not running', async () => {
    const closed = await workspace.runCli(['dashboard', 'close']);
    expect(closed.stdoutAsString()).toMatch(/Stopped the Candle dashboard \(pid \d+\)/);

    const all = await workspace.runCli(['list-all']);
    expect(all.stdoutAsString()).not.toMatch(/^dashboard\s/m);
    await expect(request(port, '/api/processes')).rejects.toThrow();

    const again = await workspace.runCli(['dashboard', 'close']);
    expect(again.stdoutAsString()).toContain('The Candle dashboard is not running');
  });
});
