#!/usr/bin/env node

// Server that reads from stdin and echoes to stdout
console.log('Stdin echo server started');

process.stdin.setEncoding('utf8');

process.stdin.on('data', (data) => {
  console.log(`[RECEIVED] ${data.trim()}`);
});

process.stdin.on('end', () => {
  console.log('Stdin closed');
});

// Handle graceful shutdown
process.on('SIGINT', () => {
  console.log('Received SIGINT, shutting down...');
  process.exit(0);
});

process.on('SIGTERM', () => {
  console.log('Received SIGTERM, shutting down...');
  process.exit(0);
});

// Keep the process running
setInterval(() => {
  // Keep alive
}, 10000);
