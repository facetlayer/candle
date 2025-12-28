console.log('Backend server started');
setInterval(() => console.log('Backend running...'), 1000);
process.on('SIGTERM', () => process.exit(0));
process.on('SIGINT', () => process.exit(0));
