#!/usr/bin/env node

// A server that starts, waits briefly, then exits
// Used to test watchProcess behavior when a process exits

const exitCode = parseInt(process.argv[2] || '0', 10);
const delayMs = parseInt(process.argv[3] || '1000', 10);

console.log(`Delayed exit server starting (will exit with code ${exitCode} after ${delayMs}ms)...`);
console.log('Delayed exit server running...');

setTimeout(() => {
    console.log(`Delayed exit server exiting with code ${exitCode}`);
    process.exit(exitCode);
}, delayMs);
