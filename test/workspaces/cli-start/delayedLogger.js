#!/usr/bin/env node

console.log('Server initializing...');

setTimeout(() => {
    console.log('Server started');
}, 2000);

setTimeout(() => {
    console.log('Server ready to accept connections');
}, 4000);

// Keep the process running
setInterval(() => {
    // Do nothing, just keep running
}, 1000);

// Handle graceful shutdown
process.on('SIGTERM', () => {
    console.log('SIGTERM received, shutting down');
    process.exit(0);
});

process.on('SIGINT', () => {
    console.log('SIGINT received, shutting down');
    process.exit(0);
});