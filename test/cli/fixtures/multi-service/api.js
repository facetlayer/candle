console.log('API server started');
setInterval(() => console.log('API handling requests...'), 1000);
process.on('SIGTERM', () => process.exit(0));
process.on('SIGINT', () => process.exit(0));
