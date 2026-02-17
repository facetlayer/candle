import { config } from 'dotenv';
config({ path: new URL('../.env', import.meta.url).pathname });

import { App, startServer } from '@facetlayer/prism-framework-api';
import { candleService } from './services/candle-service.ts';

const PORT = parseInt(process.env.PRISM_API_PORT || '4800', 10);

async function main() {
  const app = new App({
    name: 'Candle GUI',
    description: 'Web interface for managing Candle processes',
    services: [candleService],
  });

  await startServer({
    port: PORT,
    app,
    corsConfig: {
      enableTestEndpoints: true,
    },
  });

  console.log(`Candle GUI API running at http://localhost:${PORT}`);
}

main().catch(console.error);
