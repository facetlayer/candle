#!/usr/bin/env node

// Echo server that outputs to both stdout and stderr
const readline = require('readline');

console.log('Echo server started');
console.error('This is stderr output from echo server');

let lineCount = 0;

const rl = readline.createInterface({
    input: process.stdin,
    output: process.stdout
});

// Echo every second
setInterval(() => {
    lineCount++;
    console.log(`[stdout] Echo ${lineCount}: The time is ${new Date().toISOString()}`);
    if (lineCount % 3 === 0) {
        console.error(`[stderr] Echo ${lineCount}: This is an error message`);
    }
}, 1000);

// Handle graceful shutdown
process.on('SIGINT', () => {
    console.log('\nReceived SIGINT, shutting down...');
    rl.close();
    process.exit(0);
});

process.on('SIGTERM', () => {
    console.log('\nReceived SIGTERM, shutting down...');
    rl.close();
    process.exit(0);
});