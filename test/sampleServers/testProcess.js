#!/usr/bin/env node

// Simple test process - no web server needed
console.log('Test server started successfully');

let running = true;
let messageCount = 0;

// Log a message every 2 seconds
const interval = setInterval(() => {
    if (running) {
        messageCount++;
        console.log(`Test message ${messageCount}: Process is running`);
    }
}, 2000);

// Handle graceful shutdown
function shutdown(signal) {
    console.log(`\nReceived ${signal}, shutting down gracefully...`);
    running = false;
    clearInterval(interval);
    console.log('Process stopped');
    process.exit(0);
}

process.on('SIGINT', () => shutdown('SIGINT'));
process.on('SIGTERM', () => shutdown('SIGTERM'));

// Keep the process running
process.stdin.resume();
