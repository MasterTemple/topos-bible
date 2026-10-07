/** One line of `topos -m index`: a searched file and its entry for the index (base64) */
interface IndexLine {
  path: string;
  entry: string;
}

export class OutdatedCliError extends Error {
  constructor() {
    super("This version of topos can't write index entries (-m index); update it with cargo install");
  }
}

/** A path from the CLI (run in the vault folder) as a vault path */
export function vaultPath(path: string): string {
  return path.replace(/^\.[\\/]/, "").replace(/\\/g, "/");
}

export function base64Bytes(text: string): Uint8Array {
  const buffer = (globalThis as { Buffer?: { from(text: string, encoding: string): Uint8Array } }).Buffer;
  if (buffer) return new Uint8Array(buffer.from(text, "base64"));
  const binary = atob(text);
  const bytes = new Uint8Array(binary.length);
  for (let i = 0; i < binary.length; i++) bytes[i] = binary.charCodeAt(i);
  return bytes;
}

/** Parses one line of `topos -m index` */
export function parseIndexLine(line: string): { path: string; bytes: Uint8Array } | null {
  if (!line.trim()) return null;
  const parsed = JSON.parse(line) as IndexLine;
  return { path: vaultPath(parsed.path), bytes: base64Bytes(parsed.entry) };
}

/** Collects streamed output into lines, each a file's entry */
export class CliOutputParser {
  private pending: string[] = [];
  private readonly onEntry: (path: string, bytes: Uint8Array) => void;

  constructor(onEntry: (path: string, bytes: Uint8Array) => void) {
    this.onEntry = onEntry;
  }

  /** Only the new chunk is scanned for line breaks, so long lines arriving in pieces stay linear */
  push(chunk: string): void {
    let start = 0;
    for (let end = chunk.indexOf("\n"); end !== -1; end = chunk.indexOf("\n", start)) {
      this.pending.push(chunk.slice(start, end));
      this.line(this.pending.join(""));
      this.pending = [];
      start = end + 1;
    }
    if (start < chunk.length) this.pending.push(chunk.slice(start));
  }

  finish(): void {
    if (this.pending.length > 0) this.line(this.pending.join(""));
    this.pending = [];
  }

  private line(line: string): void {
    const entry = parseIndexLine(line);
    if (entry) this.onEntry(entry.path, entry.bytes);
  }
}

/** Whether the CLI reported that it couldn't search a file (`topos: <path>: <error>` on stderr) */
export function cliFailed(errors: string[], path: string): boolean {
  return errors.some((line) => {
    const normal = line.replace(/\\/g, "/");
    return normal.startsWith(`topos: ${path}: `) || normal.startsWith(`topos: ./${path}: `);
  });
}

/**
 * Splits paths into batches for the CLI's command line: at most `maxChars` (Windows allows 32,767
 * characters in all)
 */
export function cliBatches(paths: string[], maxChars = 16_000): string[][] {
  const batches: string[][] = [];
  let batch: string[] = [];
  let chars = 0;
  for (const path of paths) {
    if (batch.length > 0 && chars + path.length + 3 > maxChars) {
      batches.push(batch);
      batch = [];
      chars = 0;
    }
    batch.push(path);
    chars += path.length + 3;
  }
  if (batch.length > 0) batches.push(batch);
  return batches;
}
