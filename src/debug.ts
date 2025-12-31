import * as fs from 'fs';
import * as path from 'path';

export function debugLog(message:string) {
    if (process.env.CANDLE_ENABLE_LOGS) {
        fs.appendFileSync(path.join(process.cwd(), 'candle.log'), message + '\n');
    }
}