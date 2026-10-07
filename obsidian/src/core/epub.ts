import type { Hit } from "./search.ts";

/** A section of an EPUB's text (one spine item), as EPUB++ extracts it */
export interface EpubSection {
  spineIndex: number;
  text: string;
  title: string | null;
}

/** The key a section is searched under: `book.epub#3` */
export function sectionKey(path: string, spineIndex: number): string {
  return `${path}#${spineIndex}`;
}

/**
 * Turns the hits in each section into the book's: offsets continue from section to section (so
 * sorting by position is book order), and each hit gets its CFI. Hits without one are dropped.
 */
export function bookHits(
  path: string,
  sections: EpubSection[],
  results: { path: string; hits: Hit[] }[],
  cfi: (spineIndex: number, start: number, end: number) => string | null,
): Hit[] {
  const bySection = new Map(results.map((r) => [r.path, r.hits]));
  const out: Hit[] = [];
  let base = 0;
  let lineBase = 0;
  for (const section of sections) {
    for (const hit of bySection.get(sectionKey(path, section.spineIndex)) ?? []) {
      const location = cfi(section.spineIndex, hit.start, hit.end);
      if (!location) continue;
      out.push({
        ...hit,
        path,
        start: base + hit.start,
        end: base + hit.end,
        line: lineBase + hit.line,
        epub: { spineIndex: section.spineIndex, cfi: location, chapter: section.title },
      });
    }
    base += section.text.length + 2;
    lineBase += section.text.split("\n").length + 1;
  }
  return out;
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

/** Bump when hits from an older version can't be reused */
export const EPUB_CACHE_VERSION = 1;

interface CachedBook {
  mtime: number;
  size: number;
  hits: Hit[];
}

/** Each EPUB's references, reused while the file is unchanged (extracting a book is slow) */
export class EpubCache {
  private files = new Map<string, CachedBook>();
  /** True when changed since `toJSON` */
  dirty = false;

  private readonly engine: string;

  constructor(engine: string) {
    this.engine = engine;
  }

  get(path: string, stat: { mtime: number; size: number }): Hit[] | null {
    const entry = this.files.get(path);
    return entry && entry.mtime === stat.mtime && entry.size === stat.size ? entry.hits : null;
  }

  set(path: string, stat: { mtime: number; size: number }, hits: Hit[]): void {
    this.files.set(path, { mtime: stat.mtime, size: stat.size, hits });
    this.dirty = true;
  }

  delete(path: string): void {
    this.dirty = this.files.delete(path) || this.dirty;
  }

  rename(oldPath: string, newPath: string): void {
    const entry = this.files.get(oldPath);
    if (!entry) return;
    this.files.delete(oldPath);
    this.files.set(newPath, { ...entry, hits: entry.hits.map((hit) => ({ ...hit, path: newPath })) });
    this.dirty = true;
  }

  toJSON(): unknown {
    this.dirty = false;
    return { version: EPUB_CACHE_VERSION, engine: this.engine, files: Object.fromEntries(this.files) };
  }

  /** A cache from `toJSON`'s output; empty if it's from another version or engine */
  static from(json: unknown, engine: string): EpubCache {
    const cache = new EpubCache(engine);
    const data = json as { version?: number; engine?: string; files?: Record<string, CachedBook> } | null;
    if (data?.version !== EPUB_CACHE_VERSION || data.engine !== engine || !data.files) return cache;
    for (const [path, entry] of Object.entries(data.files)) {
      if (entry && Array.isArray(entry.hits)) cache.files.set(path, entry);
    }
    return cache;
  }
}
