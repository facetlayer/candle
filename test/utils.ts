import * as path from 'path';
import * as fs from 'fs';

export function getTestDataDirectory(testName: string) {
    return path.join(__dirname, 'tempdata', testName);
}

export function clearTestData(testName: string) {
    const dir = getTestDataDirectory(testName);
    if (fs.existsSync(dir)) {
        fs.rmSync(dir, { recursive: true, force: true });
    }
    fs.mkdirSync(dir, { recursive: true });
}

export function getCandleBinPath() {
    return path.join(__dirname, '..', 'bin', 'candle');
}
