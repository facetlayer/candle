import { readFileSync, readdirSync } from 'node:fs';
import { basename, join, relative } from 'node:path';

/*
  DocFilesHelper

  A small, self-contained helper for listing and displaying the project's
  markdown documentation files. This is a trimmed reimplementation of the
  subset of @facetlayer/docs-tool that Candle actually uses (the `list-docs`
  and `get-doc` commands), kept in-repo so the build doesn't depend on a
  private GitHub Packages dependency.
*/

export interface Frontmatter {
  name?: string;
  description?: string;
  [key: string]: string | undefined;
}

export interface ParsedDocument {
  frontmatter: Frontmatter;
  content: string;
}

export interface DocInfo {
  name: string;
  description: string;
  filename: string;
}

export interface DocContent extends DocInfo {
  content: string;
  rawContent: string;
  fullPath: string;
}

export interface DocFilesHelperOptions {
  /** Directories to scan for `.md` files. */
  dirs?: string[];
  /** Individual files to include (e.g. a top-level README.md). */
  files?: string[];
  /** The CLI subcommand used to display a doc (shown in `list-docs` hints). */
  getDocSubcommand?: string;
}

/*
  parseFrontmatter

  Parses YAML-style frontmatter delimited by `---` at the start of a markdown
  document. Only simple `key: value` lines are supported.
*/
export function parseFrontmatter(text: string): ParsedDocument {
  const frontmatterRegex = /^---\r?\n([\s\S]*?)\r?\n---\r?\n([\s\S]*)$/;
  const match = text.match(frontmatterRegex);

  if (!match) {
    return { frontmatter: {}, content: text };
  }

  const [, frontmatterBlock, content] = match;
  const frontmatter: Frontmatter = {};

  for (const line of frontmatterBlock.split('\n')) {
    const colonIndex = line.indexOf(':');
    if (colonIndex === -1) continue;
    const key = line.slice(0, colonIndex).trim();
    const value = line.slice(colonIndex + 1).trim();
    frontmatter[key] = value;
  }

  return { frontmatter, content: content.trim() };
}

export class DocFilesHelper {
  options: DocFilesHelperOptions;
  private fileMap: Map<string, string> = new Map();

  constructor(options: DocFilesHelperOptions) {
    this.options = options;

    if (options.dirs) {
      for (const dir of options.dirs) {
        const files = readdirSync(dir);
        for (const file of files) {
          if (!file.endsWith('.md')) continue;
          this.fileMap.set(file, join(dir, file));
        }
      }
    }

    if (options.files) {
      for (const filePath of options.files) {
        this.fileMap.set(basename(filePath), filePath);
      }
    }
  }

  /*
    Builds the command string shown in `list-docs` output, telling the user how
    to display a given doc file. Handles both the installed `candle` binary and
    running directly via `node main-cli.ts` during development.
  */
  formatGetDocCommand(filename: string): string {
    const subcommand = this.options.getDocSubcommand || 'get-doc';
    const script = relative(process.cwd(), process.argv[1]);
    const binName = basename(script);

    if (binName === '.' || binName.endsWith('.js') || binName.endsWith('.mjs') || binName.endsWith('.ts')) {
      return `node ${script} ${subcommand} ${filename}`;
    }

    return `${binName} ${subcommand} ${filename}`;
  }

  /*
    Lists all doc files with metadata pulled from their frontmatter. Files that
    no longer exist on disk are silently skipped.
  */
  listDocs(): DocInfo[] {
    const docs: DocInfo[] = [];

    for (const [baseFilename, fullPath] of this.fileMap) {
      let rawContent: string;
      try {
        rawContent = readFileSync(fullPath, 'utf-8');
      } catch (err: any) {
        if (err.code === 'ENOENT') continue;
        throw err;
      }

      const { frontmatter } = parseFrontmatter(rawContent);
      docs.push({
        name: frontmatter.name || basename(baseFilename, '.md'),
        description: frontmatter.description || '',
        filename: baseFilename,
      });
    }

    return docs;
  }

  /*
    Returns the contents of a doc file by name. Falls back to a partial match on
    filename or frontmatter name when there's no exact match. Throws if nothing
    matches or if the name is ambiguous.
  */
  getDoc(name: string): DocContent {
    const baseName = name.endsWith('.md') ? name.slice(0, -3) : name;
    const filename = `${baseName}.md`;
    const fullPath = this.fileMap.get(filename);

    if (fullPath) {
      return this.readDoc(filename, fullPath);
    }

    const docs = this.listDocs();
    const lowerBase = baseName.toLowerCase();
    const matches = docs.filter(
      doc =>
        doc.filename.toLowerCase().includes(lowerBase) ||
        doc.name.toLowerCase().includes(lowerBase)
    );

    if (matches.length === 0) {
      throw new Error(`Doc file not found: ${baseName}`);
    }

    if (matches.length > 1) {
      const matchNames = matches.map(m => m.filename).join(', ');
      throw new Error(`Multiple docs match "${baseName}": ${matchNames}. Please be more specific.`);
    }

    const matchedFilename = matches[0].filename;
    return this.readDoc(matchedFilename, this.fileMap.get(matchedFilename)!);
  }

  private readDoc(filename: string, fullPath: string): DocContent {
    const rawContent = readFileSync(fullPath, 'utf-8');
    const { frontmatter, content } = parseFrontmatter(rawContent);
    return {
      name: frontmatter.name || basename(filename, '.md'),
      description: frontmatter.description || '',
      filename,
      content,
      rawContent,
      fullPath,
    };
  }

  /*
    Prints a formatted list of all doc files to stdout. Used by `list-docs`.
  */
  printDocFileList(): void {
    const docs = this.listDocs();
    console.log('Available doc files:\n');

    for (const doc of docs) {
      if (doc.description) {
        console.log(`  ${doc.name} (${this.formatGetDocCommand(doc.filename)}):`);
        console.log(`    ${doc.description}\n`);
      } else {
        console.log(`  ${doc.name} (${this.formatGetDocCommand(doc.filename)})\n`);
      }
    }
  }

  /*
    Prints the raw contents of a doc file to stdout. Used by `get-doc`. Exits
    with a non-zero status if the doc can't be found.
  */
  printDocFileContents(name: string): void {
    let doc: DocContent;
    try {
      doc = this.getDoc(name);
    } catch {
      console.error(`Doc file not found: ${name}`);
      console.error('Run with "list-docs" command to see available docs.');
      process.exit(1);
    }

    console.log(doc.rawContent);
    console.log(`\n(File source: ${doc.fullPath})`);
  }
}
