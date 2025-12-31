import * as path from 'path';

export interface CommandResult {
    stdout: string;
    stderr: string;
    code: number;
}

export function getTestTempDirectory(testName: string) {
    return path.join(__dirname, 'temp', testName);
}

export function getSampleServersDirectory() {
    return path.join(__dirname, 'sampleServers');
}

export function getCliPath() {
    return path.join(__dirname, '..', 'src', 'main-cli.ts');
}
