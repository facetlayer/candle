export const ProcessLogType = {
  stdout: 1,
  stderr: 2,

  // process_start_initiated - Saved immediately when we being launching a subprocess
  process_start_initiated: 3,

  // process_start_failed - Saved when the subprocess fails to start.
  process_start_failed: 4,

  // process_started - Saved when the subprocess has successfully started.
  process_started: 5,

  // process_exited - Saved when the subprocess exits.
  process_exited: 6,
} as const;
