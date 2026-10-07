import {
  IndexOrder,
  IndexScope,
  IndexStatus,
  OffsetUnit,
  Testament,
  ToposIndex,
  ToposQuery,
  type EpubReference,
  type IndexHit,
  type IndexResults,
  type Topos,
} from "topos-bible";
import { compileFilters, NO_FILTERS, type Filters } from "./filters.ts";
import type { Hit } from "./search.ts";
import type { SortOrder } from "./sort.ts";

export { IndexStatus };

/** A file's size and modification time (milliseconds), as Obsidian's `TFile.stat` has them */
export interface FileStat {
  size: number;
  mtime: number;
}

/** Which files a query looks at */
export type QueryScope = { kind: "vault" } | { kind: "file"; path: string } | { kind: "folder"; path: string };

/** A hit from the index, in the plugin's shape */
export function toHit(hit: IndexHit): Hit {
  return {
    path: hit.path,
    start: hit.start,
    end: hit.end,
    line: hit.line,
    column: hit.column,
    lineText: hit.lineText,
    passage: hit.passage,
    ...(hit.epub && { epub: { spineIndex: hit.epub.spineIndex, cfi: hit.epub.cfi, chapter: hit.epub.chapter } }),
  };
}

/** The sidebar's filters as the engine's query, leaving out names and passages it doesn't know */
export function toQuery(topos: Topos, filters: Filters): ToposQuery {
  const testament = (t: string) => (t === "old" ? Testament.Old : Testament.New);
  const book = (name: string) => topos.findBook(name) !== null;
  const genre = (name: string) => topos.findGenre(name) !== null;
  const passage = (text: string) => topos.parse(text, 0) !== null;
  let query = ToposQuery.create();
  for (const t of filters.testaments) query = query.testament(testament(t));
  for (const t of filters.excludeTestaments) query = query.excludeTestament(testament(t));
  for (const g of filters.genres.filter(genre)) query = query.genre(g);
  for (const g of filters.excludeGenres.filter(genre)) query = query.excludeGenre(g);
  for (const b of filters.books.filter(book)) query = query.book(b);
  for (const b of filters.excludeBooks.filter(book)) query = query.excludeBook(b);
  for (const p of filters.inside.filter(passage)) query = query.inside(p);
  for (const p of filters.anyOverlap.filter(passage)) query = query.anyOverlap(p);
  for (const p of filters.explicitOverlap.filter(passage)) query = query.explicitOverlap(p);
  for (const p of filters.exactOverlap.filter(passage)) query = query.exactOverlap(p);
  for (const p of filters.excludeOverlap.filter(passage)) query = query.excludeOverlap(p);
  return query;
}

/** What a query found, read a page at a time */
export class QueryResults {
  private readonly index: ReferenceIndex;
  private readonly results: IndexResults | null;

  constructor(index: ReferenceIndex, results: IndexResults | null) {
    this.index = index;
    this.results = results;
  }

  /** How many references */
  get total(): number {
    return this.results?.len() ?? 0;
  }

  /** How many files they're in */
  get files(): number {
    return this.results?.fileCount() ?? 0;
  }

  page(offset: number, limit: number): Hit[] {
    if (!this.results) return [];
    return this.index.core.page(this.index.topos, this.results, offset, limit).map(toHit);
  }

  /** The EPUB details a page needs that aren't loaded (to load, then read the page again) */
  missingDetails(offset: number, limit: number): string[] {
    return this.results ? this.index.core.missingDetails(this.results, offset, limit) : [];
  }
}

/**
 * Every reference in the vault, kept by the engine (topos-bible's `ToposIndex`): compact, so a
 * million references fit on a phone, and queried in WebAssembly a page at a time
 */
export class ReferenceIndex {
  readonly core: ToposIndex;
  readonly topos: Topos;

  constructor(topos: Topos, device = "local", engine = "") {
    this.topos = topos;
    this.core = ToposIndex.create(device, engine, OffsetUnit.Utf16);
  }

  /** Searches a file's text on this thread */
  update(path: string, text: string, stat: FileStat = { size: text.length, mtime: 0 }): void {
    this.core.indexText(this.topos, path, stat.size, stat.mtime, text, Date.now());
  }

  /** Adds an entry from a background thread or the CLI */
  insert(bytes: Uint8Array, path: string, stat: FileStat): void {
    this.core.insert(bytes, path, stat.size, stat.mtime);
  }

  setEpub(path: string, stat: FileStat, references: EpubReference[]): void {
    this.core.setEpub(path, stat.size, stat.mtime, Date.now(), references);
  }

  remove(path: string): void {
    this.core.remove(path);
  }

  rename(oldPath: string, newPath: string): void {
    this.core.rename(oldPath, newPath);
  }

  /** A file's references */
  get(path: string): Hit[] {
    return this.core.fileHits(this.topos, path).map(toHit);
  }

  /** The references the filters keep, in `scope`, sorted */
  query(filters: Filters, scope: QueryScope = { kind: "vault" }, order: SortOrder = "file"): QueryResults {
    const compiled = compileFilters(this.topos, filters);
    if (compiled.conflict) return new QueryResults(this, null);
    const kind = { vault: IndexScope.All, file: IndexScope.File, folder: IndexScope.Folder }[scope.kind];
    const path = scope.kind === "vault" ? "" : scope.path;
    const query = toQuery(this.topos, filters);
    try {
      const results = this.core.query(this.topos, query, kind, path, order === "bible" ? IndexOrder.Bible : IndexOrder.File);
      return new QueryResults(this, results);
    } finally {
      query.dispose();
    }
  }

  /** Every reference, by path and position (for tests and small vaults) */
  all(): Hit[] {
    const results = this.query(NO_FILTERS);
    return results.page(0, results.total);
  }

  get fileCount(): number {
    return this.core.fileCount();
  }

  get referenceCount(): number {
    return this.core.referenceCount();
  }
}
