#!/usr/bin/env node

// Prints `<prefix> 0` .. `<prefix> N-1` right away, then stays alive until killed.
// Usage: node burstServer.js <count> [prefix]

const count = parseInt(process.argv[2] || '10', 10);
const prefix = process.argv[3] || 'burst';

for (let i = 0; i < count; i++) {
    console.log(`${prefix} ${i}`);
}
console.log(`${prefix} done`);

setInterval(() => {}, 1000);

process.on('SIGTERM', () => process.exit(0));
