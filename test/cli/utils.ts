export { TestWorkspace, CommandResult } from '../TestWorkspace';

/**
 * Normalize output for snapshot testing
 * Removes dynamic content like timestamps, PIDs, paths, etc.
 */
export function normalizeOutput(output: string): string {
    return (
        output
            // Normalize line endings
            .replace(/\r\n/g, '\n')
            // Remove trailing whitespace from lines
            .split('\n')
            .map((line) => line.trimEnd())
            .join('\n')
            // Normalize uptime values (e.g., "0m 5s" -> "<uptime>")
            .replace(/\d+m\s+\d+s|\d+s/g, '<uptime>')
            // Normalize PIDs
            .replace(/PID:\s*\d+/g, 'PID: <pid>')
            .replace(/pid\s+\d+/gi, 'pid <pid>')
            // Normalize absolute paths to project-relative
            .replace(/\/Users\/[^\s]+\/candle\//g, '<project>/')
            .replace(/\/home\/[^\s]+\/candle\//g, '<project>/')
            .replace(/C:\\[^\s]+\\candle\\/g, '<project>/')
            // Normalize temp directory paths
            .replace(/\/tmp\/[^\s]+/g, '<tmpdir>')
            // Normalize database paths in output
            .replace(/CANDLE_DATABASE_DIR=[^\s]+/g, 'CANDLE_DATABASE_DIR=<dbdir>')
            // Trim leading/trailing whitespace
            .trim()
    );
}
