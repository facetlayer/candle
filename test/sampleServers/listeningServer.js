#!/usr/bin/env node

// HTTP server that listens on a TCP port, for tests that inspect open ports.
// The port comes from the PORT environment variable (default 8080); PORT=0
// asks the OS for a free port. Logs "Now listening on port <n>" once bound.
import http from 'http';

const PORT = Number(process.env.PORT ?? 8080);

const server = http.createServer((req, res) => {
    res.writeHead(200, { 'Content-Type': 'text/plain' });
    res.end('Hello from test server\n');
});

server.listen(PORT, '127.0.0.1', () => {
    console.log(`Now listening on port ${server.address().port}`);
});

for (const signal of ['SIGTERM', 'SIGINT']) {
    process.on(signal, () => {
        console.log(`${signal} received, shutting down gracefully`);
        server.close(() => {
            console.log('Server closed');
            process.exit(0);
        });
    });
}
