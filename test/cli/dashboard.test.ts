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

describe('CLI open-dashboard / dashboard', () => {
  let port: number;

  beforeAll(async () => {
    await workspace.runCli(['start', 'web']);
    await workspace.runCli(['wait-for-log', 'web', '--message', 'Now listening']);
    const result = await workspace.runCli(['open-dashboard', '--no-browser']);
    const match = result
      .stdoutAsString()
      .match(/Started the Candle dashboard at http:\/\/localhost:(\d+)/);
    expect(match).not.toBeNull();
    port = Number(match![1]);
  });

  afterAll(() => workspace.cleanup());

  it('reuses the running dashboard on a second call', async () => {
    const result = await workspace.runCli(['open-dashboard', '--no-browser']);
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
    const result = await workspace.runCli(['open-dashboard', '--port', '70000'], {
      ignoreExitCode: true,
    });
    expect(result.failed()).toBe(true);
    expect(result.stderrAsString()).toContain('--port must be between 1 and 65535');
  });
});
