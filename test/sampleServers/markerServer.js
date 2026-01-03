#!/usr/bin/env node

// A server that outputs a configurable marker and then exits
// Used to test multiple launches filtering
// Usage: node markerServer.js [marker] [delayMs]

const marker = process.argv[2] || 'DEFAULT_MARKER';
const delayMs = parseInt(process.argv[3] || '500', 10);

console.log(`Marker server starting with marker: ${marker}`);
console.log(`MARKER=${marker}`);

setTimeout(() => {
    console.log(`Marker server exiting`);
    process.exit(0);
}, delayMs);
