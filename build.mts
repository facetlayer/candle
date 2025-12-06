#! /usr/bin/env node

import { runBuildTool } from '@facetlayer/build-config-nodejs';

runBuildTool({
  entryPoints: [
    'src/index.ts',
    'src/main-cli.ts',
    'src/main-log-collector.ts',
  ],
});
