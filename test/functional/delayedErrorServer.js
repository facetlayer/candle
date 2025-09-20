#!/usr/bin/env node

console.log('Starting delayed error server...');
console.log('Server initializing...');
console.log('Loading configuration...');

// Simulate some initialization work for 3 seconds
setTimeout(() => {
    console.error('ERROR: Failed to connect to database after timeout');
    console.error('Server startup failed');
    process.exit(1);
}, 3000);

console.log('Waiting for database connection...');