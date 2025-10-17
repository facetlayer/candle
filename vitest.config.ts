import { defineConfig } from 'vitest/config';
import path from 'path';

export default defineConfig({
    test: {
        globals: true,
        environment: 'node',
        testTimeout: 30000, // 30 seconds for subprocess tests
        hookTimeout: 10000,
        setupFiles: ['test/setup.ts'],
    },
});
