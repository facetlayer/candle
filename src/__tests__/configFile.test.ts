import { describe, it, expect } from 'vitest';
import { validateConfig, getLogEvictionConfig, LOG_EVICTION_DEFAULTS } from '../configFile.ts';
import { ConfigFileError } from '../errors.ts';

describe('Config File Validation', () => {
    it('should validate normal array-style services configuration', () => {
        const config = {
            services: [
                { name: 'web', shell: 'npm start' },
                { name: 'api', shell: 'node server.js', root: 'backend' }
            ]
        };

        const result = validateConfig(config);
        expect(result.services).toHaveLength(2);
        expect(result.services[0].name).toBe('web');
        expect(result.services[0].shell).toBe('npm start');
        expect(result.services[1].name).toBe('api');
        expect(result.services[1].shell).toBe('node server.js');
        expect(result.services[1].root).toBe('backend');
    });

    it('should convert object-style services configuration to array', () => {
        const config: any = {
            services: {
                web: { shell: 'npm start' },
                api: { shell: 'node server.js', root: 'backend' }
            }
        };

        const result = validateConfig(config);
        expect(result.services).toHaveLength(2);

        const webService = result.services.find(s => s.name === 'web');
        const apiService = result.services.find(s => s.name === 'api');

        expect(webService).toBeDefined();
        expect(webService!.shell).toBe('npm start');

        expect(apiService).toBeDefined();
        expect(apiService!.shell).toBe('node server.js');
        expect(apiService!.root).toBe('backend');
    });

    it('should handle object-style services with mixed properties', () => {
        const config: any = {
            services: {
                frontend: { shell: 'npm run dev', root: 'client' },
                backend: { shell: 'python manage.py runserver' },
                worker: { shell: 'celery worker', root: 'tasks' }
            }
        };

        const result = validateConfig(config);
        expect(result.services).toHaveLength(3);

        const services = result.services.reduce((acc, service) => {
            acc[service.name] = service;
            return acc;
        }, {} as Record<string, any>);

        expect(services.frontend.shell).toBe('npm run dev');
        expect(services.frontend.root).toBe('client');
        expect(services.backend.shell).toBe('python manage.py runserver');
        expect(services.backend.root).toBeUndefined();
        expect(services.worker.shell).toBe('celery worker');
        expect(services.worker.root).toBe('tasks');
    });

    it('should throw error for invalid services type', () => {
        const config: any = {
            services: 'invalid'
        };

        expect(() => validateConfig(config)).toThrow(ConfigFileError);
        expect(() => validateConfig(config)).toThrow('Invalid value for \'services\'');
    });

    it('should throw error for number services', () => {
        const config: any = {
            services: 123
        };

        expect(() => validateConfig(config)).toThrow(ConfigFileError);
        expect(() => validateConfig(config)).toThrow('Invalid value for \'services\'');
    });

    it('should handle empty object services', () => {
        const config: any = {
            services: {}
        };

        const result = validateConfig(config);
        expect(result.services).toHaveLength(0);
    });

    it('should validate object-style services still require shell property', () => {
        const config: any = {
            services: {
                web: { name: 'web' } // missing shell
            }
        };

        expect(() => validateConfig(config)).toThrow(ConfigFileError);
        expect(() => validateConfig(config)).toThrow('must have a "shell" string');
    });

    it('should detect duplicate names in object-style services', () => {
        const config: any = {
            services: {
                web: { shell: 'npm start' },
                api: { shell: 'node server.js' }
            }
        };

        // This test verifies that validation still works for duplicates
        // In object-style, duplicate keys would be impossible due to JS object nature
        // but we still want to test the validation logic
        const result = validateConfig(config);
        expect(result.services).toHaveLength(2);

        // Manually test duplicate detection by creating an array with duplicates
        const configWithDuplicates = {
            services: [
                { name: 'web', shell: 'npm start' },
                { name: 'web', shell: 'npm run dev' }
            ]
        };

        expect(() => validateConfig(configWithDuplicates)).toThrow(ConfigFileError);
        expect(() => validateConfig(configWithDuplicates)).toThrow('Duplicate service name: "web"');
    });

    it('should validate root paths in object-style services', () => {
        const configWithInvalidRoot: any = {
            services: {
                web: { shell: 'npm start', root: '../invalid' }
            }
        };

        expect(() => validateConfig(configWithInvalidRoot)).toThrow(ConfigFileError);
        expect(() => validateConfig(configWithInvalidRoot)).toThrow('invalid root path');
    });
});

describe('logEviction config validation', () => {
    it('should accept valid logEviction config', () => {
        const config = {
            services: [{ name: 'web', shell: 'npm start' }],
            logEviction: {
                maxLogsPerService: 500,
                maxRetentionSeconds: 3600,
            }
        };

        const result = validateConfig(config);
        expect(result.logEviction).toEqual({
            maxLogsPerService: 500,
            maxRetentionSeconds: 3600,
        });
    });

    it('should accept config without logEviction', () => {
        const config = {
            services: [{ name: 'web', shell: 'npm start' }],
        };

        const result = validateConfig(config);
        expect(result.logEviction).toBeUndefined();
    });

    it('should accept partial logEviction config', () => {
        const config = {
            services: [{ name: 'web', shell: 'npm start' }],
            logEviction: {
                maxLogsPerService: 2000,
            }
        };

        const result = validateConfig(config);
        expect(result.logEviction!.maxLogsPerService).toBe(2000);
        expect(result.logEviction!.maxRetentionSeconds).toBeUndefined();
    });

    it('should throw for non-object logEviction', () => {
        const config: any = {
            services: [{ name: 'web', shell: 'npm start' }],
            logEviction: 'invalid',
        };

        expect(() => validateConfig(config)).toThrow(ConfigFileError);
        expect(() => validateConfig(config)).toThrow("Invalid value for 'logEviction'");
    });

    it('should throw for array logEviction', () => {
        const config: any = {
            services: [{ name: 'web', shell: 'npm start' }],
            logEviction: [1, 2, 3],
        };

        expect(() => validateConfig(config)).toThrow(ConfigFileError);
        expect(() => validateConfig(config)).toThrow("Invalid value for 'logEviction'");
    });

    it('should throw for non-integer maxLogsPerService', () => {
        const config: any = {
            services: [{ name: 'web', shell: 'npm start' }],
            logEviction: { maxLogsPerService: 1.5 },
        };

        expect(() => validateConfig(config)).toThrow(ConfigFileError);
        expect(() => validateConfig(config)).toThrow('maxLogsPerService');
    });

    it('should throw for zero maxLogsPerService', () => {
        const config: any = {
            services: [{ name: 'web', shell: 'npm start' }],
            logEviction: { maxLogsPerService: 0 },
        };

        expect(() => validateConfig(config)).toThrow(ConfigFileError);
        expect(() => validateConfig(config)).toThrow('maxLogsPerService');
    });

    it('should throw for negative maxLogsPerService', () => {
        const config: any = {
            services: [{ name: 'web', shell: 'npm start' }],
            logEviction: { maxLogsPerService: -10 },
        };

        expect(() => validateConfig(config)).toThrow(ConfigFileError);
        expect(() => validateConfig(config)).toThrow('maxLogsPerService');
    });

    it('should throw for string maxLogsPerService', () => {
        const config: any = {
            services: [{ name: 'web', shell: 'npm start' }],
            logEviction: { maxLogsPerService: '1000' },
        };

        expect(() => validateConfig(config)).toThrow(ConfigFileError);
        expect(() => validateConfig(config)).toThrow('maxLogsPerService');
    });

    it('should throw for non-integer maxRetentionSeconds', () => {
        const config: any = {
            services: [{ name: 'web', shell: 'npm start' }],
            logEviction: { maxRetentionSeconds: 10.5 },
        };

        expect(() => validateConfig(config)).toThrow(ConfigFileError);
        expect(() => validateConfig(config)).toThrow('maxRetentionSeconds');
    });

    it('should throw for zero maxRetentionSeconds', () => {
        const config: any = {
            services: [{ name: 'web', shell: 'npm start' }],
            logEviction: { maxRetentionSeconds: 0 },
        };

        expect(() => validateConfig(config)).toThrow(ConfigFileError);
        expect(() => validateConfig(config)).toThrow('maxRetentionSeconds');
    });

    it('should throw for negative maxRetentionSeconds', () => {
        const config: any = {
            services: [{ name: 'web', shell: 'npm start' }],
            logEviction: { maxRetentionSeconds: -3600 },
        };

        expect(() => validateConfig(config)).toThrow(ConfigFileError);
        expect(() => validateConfig(config)).toThrow('maxRetentionSeconds');
    });
});

describe('getLogEvictionConfig', () => {
    it('should return defaults when no config provided', () => {
        const result = getLogEvictionConfig();
        expect(result.maxLogsPerService).toBe(LOG_EVICTION_DEFAULTS.maxLogsPerService);
        expect(result.maxRetentionSeconds).toBe(LOG_EVICTION_DEFAULTS.maxRetentionSeconds);
    });

    it('should return defaults when config has no logEviction', () => {
        const result = getLogEvictionConfig({ services: [] });
        expect(result.maxLogsPerService).toBe(LOG_EVICTION_DEFAULTS.maxLogsPerService);
        expect(result.maxRetentionSeconds).toBe(LOG_EVICTION_DEFAULTS.maxRetentionSeconds);
    });

    it('should override defaults with config values', () => {
        const result = getLogEvictionConfig({
            services: [],
            logEviction: {
                maxLogsPerService: 500,
                maxRetentionSeconds: 3600,
            }
        });
        expect(result.maxLogsPerService).toBe(500);
        expect(result.maxRetentionSeconds).toBe(3600);
    });

    it('should partially override defaults', () => {
        const result = getLogEvictionConfig({
            services: [],
            logEviction: {
                maxLogsPerService: 2000,
            }
        });
        expect(result.maxLogsPerService).toBe(2000);
        expect(result.maxRetentionSeconds).toBe(LOG_EVICTION_DEFAULTS.maxRetentionSeconds);
    });
});