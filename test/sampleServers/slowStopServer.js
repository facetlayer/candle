#!/usr/bin/env node

// A server that shuts down slowly and noisily, used to reproduce the race where
// a restarting service's *previous* instance writes log rows (its shutdown
// output and its `process_exited` row) after the new launch has already been
// recorded. Watch mode must not display any of it.

console.log('Slow stop server started');

const interval = setInterval(() => {}, 1000);

process.on('SIGTERM', () => {
    console.log('PREVIOUS-INSTANCE-SHUTDOWN-MARKER');
    setTimeout(() => {
        clearInterval(interval);
        // Die from a signal so the monitor records the "Process was stopped"
        // flavor of the exit row, matching a real `candle run` restart.
        process.kill(process.pid, 'SIGKILL');
    }, 800);
});
