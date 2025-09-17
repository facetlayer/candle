#!/usr/bin/env node

// Simple HTTP server for testing
const http = require('http');

const port = process.env.PORT || 3000;
let requestCount = 0;

const server = http.createServer((req, res) => {
    requestCount++;
    console.log(`[${new Date().toISOString()}] Request ${requestCount}: ${req.method} ${req.url}`);
    
    res.writeHead(200, { 'Content-Type': 'text/plain' });
    res.end(`Hello from test server! Request count: ${requestCount}\n`);
});

server.listen(port, () => {
    console.log(`Test server listening on port ${port}`);
    console.log('Press Ctrl+C to stop');
});

// Handle graceful shutdown
process.on('SIGINT', () => {
    console.log('\nReceived SIGINT, shutting down gracefully...');
    server.close(() => {
        console.log('Server closed');
        process.exit(0);
    });
});

process.on('SIGTERM', () => {
    console.log('\nReceived SIGTERM, shutting down gracefully...');
    server.close(() => {
        console.log('Server closed');
        process.exit(0);
    });
});