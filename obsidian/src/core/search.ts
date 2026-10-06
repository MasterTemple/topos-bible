import { OffsetUnit, type Passage, type Topos } from "topos-bible";

/** A reference found in a file; offsets and columns are UTF-16, like CodeMirror's */
export interface Hit {
  path: string;
  start: number;
  end: number;
  /** 1-based */
  line: number;
  /** 1-based */
  column: number;
  /** The line the reference starts on, for context */
  lineText: string;
  passage: Passage;
}

export function searchText(topos: Topos, path: string, text: string): Hit[] {
  return topos.search(text, OffsetUnit.Utf16).map((m) => {
    const lineStart = text.lastIndexOf("\n", m.start - 1) + 1;
    const lineEnd = text.indexOf("\n", m.start);
    return {
      path,
      start: m.start,
      end: m.end,
      line: m.line,
      column: m.column,
      lineText: text.slice(lineStart, lineEnd === -1 ? text.length : lineEnd),
      passage: m.passage,
    };
  });
}

/** Every reference in the vault by file, kept up to date as files change */
export class ReferenceIndex {
  private files = new Map<string, Hit[]>();
  private readonly topos: Topos;

  constructor(topos: Topos) {
    this.topos = topos;
  }

  /** Searches a file's text on this thread (the background indexer uses `set` instead) */
  update(path: string, text: string): void {
    this.set(path, searchText(this.topos, path, text));
  }

  set(path: string, hits: Hit[]): void {
    if (hits.length === 0) this.files.delete(path);
    else this.files.set(path, hits);
  }

  remove(path: string): void {
    this.files.delete(path);
  }

  rename(oldPath: string, newPath: string): void {
    const hits = this.files.get(oldPath);
    if (!hits) return;
    this.files.delete(oldPath);
    this.files.set(
      newPath,
      hits.map((hit) => ({ ...hit, path: newPath })),
    );
  }

  get(path: string): Hit[] {
    return this.files.get(path) ?? [];
  }

  /** Every hit, by path and then position */
  all(): Hit[] {
    return [...this.files.keys()].sort().flatMap((path) => this.get(path));
  }

  get fileCount(): number {
    return this.files.size;
  }
}
