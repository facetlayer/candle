#!/usr/bin/env node

console.log('Starting error server...');
console.error('ERROR: This is a test error message');
console.error('Failed to initialize server');

// Exit with error code 1
process.exit(1);