import type { Passage, PassageSegment } from "topos-bible";
import type { Hit } from "./search.ts";

/** One line of `topos -m json` output (only the fields the plugin uses) */
interface CliMatch {
  path?: string;
  reference: string;
  osis: string;
  book_id: number;
  book: string;
  segments: PassageSegment[];
  line: number;
  utf16_column: number;
  start_utf16: number;
  end_utf16: number;
  line_text: string;
}

export class OutdatedCliError extends Error {
  constructor() {
    super("This version of topos does not report UTF-16 positions; update it with cargo install");
  }
}

/** Parses one line of `topos -m json` (run in the vault folder) into a hit */
export function parseCliLine(line: string): Hit | null {
  if (!line.trim()) return null;
  const m = JSON.parse(line) as CliMatch;
  if (m.start_utf16 === undefined || m.segments === undefined) throw new OutdatedCliError();
  const passage: Passage = {
    bookId: m.book_id,
    book: m.book,
    reference: m.reference,
    segments: m.segments,
    osis: m.osis,
  };
  return {
    path: (m.path ?? "").replace(/^\.\//, "").replace(/\\/g, "/"),
    start: m.start_utf16,
    end: m.end_utf16,
    line: m.line,
    column: m.utf16_column,
    lineText: m.line_text,
    passage,
  };
}

/**
 * Collects streamed output into complete lines, and hits into files: the CLI prints each file's
 * hits together, so a file is done when the next one starts
 */
export class CliOutputParser {
  private buffer = "";
  private path: string | null = null;
  private hits: Hit[] = [];
  private readonly onFile: (path: string, hits: Hit[]) => void;

  constructor(onFile: (path: string, hits: Hit[]) => void) {
    this.onFile = onFile;
  }

  push(chunk: string): void {
    this.buffer += chunk;
    const lines = this.buffer.split("\n");
    this.buffer = lines.pop() ?? "";
    for (const line of lines) this.line(line);
  }

  finish(): void {
    if (this.buffer) this.line(this.buffer);
    this.buffer = "";
    this.flush();
  }

  private line(line: string): void {
    const hit = parseCliLine(line);
    if (!hit) return;
    if (hit.path !== this.path) {
      this.flush();
      this.path = hit.path;
    }
    this.hits.push(hit);
  }

  private flush(): void {
    if (this.path !== null && this.hits.length > 0) this.onFile(this.path, this.hits);
    this.path = null;
    this.hits = [];
  }
}
