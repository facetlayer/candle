import { unixPipeToLines } from '@facetlayer/parse-stdout-lines';

/**
 * Reads a single JSON message from stdin.
 *
 * Uses unixPipeToLines to correctly handle newline-delimited input,
 * parsing the first complete line as JSON. Resolves once a valid
 * JSON message is received, or rejects if stdin closes first.
 *
 * @returns Promise that resolves with the parsed JSON message
 */
export function readStdinAsJson<T = unknown>(): Promise<T> {
  return new Promise((resolve, reject) => {
    let resolved = false;

    const cleanup = unixPipeToLines(process.stdin, (line: string | null) => {
      if (resolved) return;

      if (line === null) {
        // Stream closed before we got a message
        resolved = true;
        reject(new Error('stdin closed before receiving any JSON message'));
        return;
      }

      try {
        const parsed = JSON.parse(line) as T;
        resolved = true;
        cleanup();
        resolve(parsed);
      } catch (error) {
        // Log parse error but keep waiting for valid JSON
        console.error(`Failed to parse JSON from stdin: ${error}`);
        console.error(`Line content: ${line}`);
      }
    });

    process.stdin.on('error', error => {
      if (!resolved) {
        resolved = true;
        cleanup();
        reject(new Error(`Failed to read stdin: ${error}`));
      }
    });
  });
}
