console.log('Scheduler started');
setInterval(() => console.log('Scheduler running...'), 1000);
process.on('SIGTERM', () => process.exit(0));
process.on('SIGINT', () => process.exit(0));
