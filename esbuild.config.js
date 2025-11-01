import { build } from 'esbuild';

const commonConfig = {
  bundle: true,
  platform: 'node',
  target: 'node16',
  format: 'esm',
  outdir: 'dist',
  external: [
    'better-sqlite3',
    '@facetlayer/sqlite-wrapper',
    '@facetlayer/subprocess-wrapper',
    'tree-kill',
    'child_process',
    'fs',
    'path',
    'os',
    'util'
  ]
};

const entryPoints = [
  'src/index.ts',
  'src/main-cli.ts',
  'src/main-log-collector.ts',
];

try {
  await build({
    ...commonConfig,
    entryPoints
  });
  console.log('Build completed successfully');
} catch (error) {
  console.error('Build failed:', error);
  process.exit(1);
}
