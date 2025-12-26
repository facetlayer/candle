import { describe, it, expect, beforeAll, afterEach } from 'vitest';
import * as path from 'path';
import * as fs from 'fs';
import { createRunCandleCommand, getTestTempDirectory, getSampleServersDirectory } from './utils';

const TEST_STATE_DIR = getTestTempDirectory('add-service-db');
const SAMPLE_SERVERS_DIR = getSampleServersDirectory();
const runCandleCommand = createRunCandleCommand(TEST_STATE_DIR);

describe('Add Service', () => {
  beforeAll(() => {
    if (fs.existsSync(TEST_STATE_DIR)) {
      fs.rmSync(TEST_STATE_DIR, { recursive: true, force: true });
    }
    fs.mkdirSync(TEST_STATE_DIR, { recursive: true });
  });

  afterEach(async () => {
    await runCandleCommand(['kill-all']).catch(() => {});
  });

  it('should add a service to an existing .candle-setup.json file', async () => {
    const tempDir = path.join(SAMPLE_SERVERS_DIR, 'add-service-test');
    if (fs.existsSync(tempDir)) {
      fs.rmSync(tempDir, { recursive: true, force: true });
    }
    fs.mkdirSync(tempDir, { recursive: true });

    const configPath = path.join(tempDir, '.candle-setup.json');
    fs.writeFileSync(configPath, JSON.stringify({ services: [] }, null, 2));

    const result = await runCandleCommand(['add-service', 'test-service', 'echo hello'], {
      cwd: tempDir,
    });

    expect(result.code).toBe(0);
    expect(result.stdout).toContain("Service 'test-service' added successfully");

    const updatedConfig = JSON.parse(fs.readFileSync(configPath, 'utf8'));
    expect(updatedConfig.services).toHaveLength(1);
    expect(updatedConfig.services[0].name).toBe('test-service');
    expect(updatedConfig.services[0].shell).toBe('echo hello');
  });

  it('should create .candle.json when no config file exists', async () => {
    const tempDir = getTestTempDirectory('add-service-no-config-test');
    if (fs.existsSync(tempDir)) {
      fs.rmSync(tempDir, { recursive: true, force: true });
    }
    fs.mkdirSync(tempDir, { recursive: true });

    const configPath = path.join(tempDir, '.candle.json');

    expect(fs.existsSync(configPath)).toBe(false);

    const result = await runCandleCommand(['add-service', 'my-service', 'npm run dev'], {
      cwd: tempDir,
    });

    expect(result.code).toBe(0);
    expect(result.stdout).toContain("Service 'my-service' added successfully");

    expect(fs.existsSync(configPath)).toBe(true);
    const config = JSON.parse(fs.readFileSync(configPath, 'utf8'));
    expect(config.services).toHaveLength(1);
    expect(config.services[0].name).toBe('my-service');
    expect(config.services[0].shell).toBe('npm run dev');

    fs.rmSync(tempDir, { recursive: true, force: true });
  });
});
