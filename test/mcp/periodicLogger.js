#!/usr/bin/env node

let counter = 0;

console.log('Starting periodic logger...');

// Output a log every 500ms
const interval = setInterval(() => {
    counter++;
    console.log(`Log message ${counter} at ${new Date().toISOString()}`);
    
    // Also output to stderr occasionally
    if (counter % 3 === 0) {
        console.error(`Error log ${counter}`);
    }
}, 500);

// Handle graceful shutdown
process.on('SIGTERM', () => {
    console.log('SIGTERM received, stopping logger');
    clearInterval(interval);
    process.exit(0);
});

process.on('SIGINT', () => {
    console.log('SIGINT received, stopping logger');
    clearInterval(interval);
    process.exit(0);
});