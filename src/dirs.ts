import Path from 'path';
import { fileURLToPath } from 'url';

const __dirname = Path.dirname(fileURLToPath(import.meta.url));
export const ProjectRootDir = Path.join(__dirname, '..');
