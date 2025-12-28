console.log('Worker started');
setInterval(() => console.log('Worker processing...'), 1000);
process.on('SIGTERM', () => process.exit(0));
process.on('SIGINT', () => process.exit(0));
