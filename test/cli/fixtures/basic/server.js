// Simple test server
console.log('Server started');

// Keep process running
setInterval(() => {
    console.log('Server running...');
}, 1000);

process.on('SIGTERM', () => {
    console.log('Server shutting down');
    process.exit(0);
});

process.on('SIGINT', () => {
    console.log('Server interrupted');
    process.exit(0);
});
