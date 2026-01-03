import express from 'express';
import { handleList, getProcessLogs, findConfigFile, AfterProcessStartLogFilter } from '@facetlayer/candle';
import path from 'path';
import { fileURLToPath } from 'url';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

export async function startApiServer(): Promise<number> {
  const app = express();

  // Serve static files from public directory
  app.use(express.static(path.join(__dirname, '../public')));

  // API endpoints
  app.get('/api/processes', async (req, res) => {
    try {
      const showAll = req.query.showAll === 'true';
      const result = await handleList({ showAll });
      res.json(result);
    } catch (error) {
      console.error('Error fetching processes:', error);
      res.status(500).json({ error: 'Failed to fetch processes' });
    }
  });

  app.get('/api/logs/:serviceName', async (req, res) => {
    try {
      const serviceName = req.params.serviceName;

      const configResult = findConfigFile(process.cwd());
      if (!configResult) {
        res.status(404).json({ error: 'No config file found' });
        return;
      }

      const { projectDir } = configResult;

      const allLogs = getProcessLogs({
        commandNames: [serviceName],
        limit: 100,
        projectDir,
      });

      const logFilter = new AfterProcessStartLogFilter();
      const logs = logFilter.filter(allLogs);

      res.json({
        logs: logs.map(log => ({
          timestamp: log.timestamp,
          content: log.content,
          type: log.log_type === 1 ? 'stdout' : log.log_type === 2 ? 'stderr' : 'event',
        })),
        serviceName,
      });
    } catch (error) {
      console.error('Error fetching logs:', error);
      res.status(500).json({ error: 'Failed to fetch logs' });
    }
  });

  // Start server on random available port
  return new Promise((resolve) => {
    const server = app.listen(0, () => {
      const address = server.address();
      const port = typeof address === 'object' && address ? address.port : 3000;
      resolve(port);
    });
  });
}
