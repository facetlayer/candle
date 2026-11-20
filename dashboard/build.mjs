// Bundle the dashboard UI into ../rust/assets/dashboard/, where the Rust
// binary embeds it with include_str!. Run `pnpm build` after editing src/ and
// commit the output: release builds compile the committed bundle and never run
// Node.
import * as esbuild from 'esbuild';
import { copyFileSync, mkdirSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));
const outdir = join(here, '..', 'rust', 'assets', 'dashboard');
const watch = process.argv.includes('--watch');

mkdirSync(outdir, { recursive: true });
for (const file of ['index.html', 'style.css']) {
    copyFileSync(join(here, 'src', file), join(outdir, file));
}

const options = {
    entryPoints: [join(here, 'src', 'main.jsx')],
    outfile: join(outdir, 'app.js'),
    bundle: true,
    minify: !watch,
    format: 'iife',
    target: 'es2020',
    jsx: 'automatic',
    define: { 'process.env.NODE_ENV': watch ? '"development"' : '"production"' },
    legalComments: 'none',
    logLevel: 'info',
};

if (watch) {
    const ctx = await esbuild.context(options);
    await ctx.watch();
} else {
    await esbuild.build(options);
}
