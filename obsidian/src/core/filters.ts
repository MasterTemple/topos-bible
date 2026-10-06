import type { Passage, Topos } from "topos-bible";

export type Testament = "old" | "new";

/** The search filters, like the CLI's options */
export interface Filters {
  /** Including any testament, genre, or book excludes everything else in that category */
  testaments: Testament[];
  genres: string[];
  books: string[];
  excludeTestaments: Testament[];
  excludeGenres: string[];
  excludeBooks: string[];
  /** Keep references entirely inside one of these passages (like `-i`) */
  inside: string[];
  /** Keep references that share a verse with one of these passages (like `-o`) */
  overlaps: string[];
  /** Drop references that share a verse with any of these passages (like `--outside`) */
  outside: string[];
}

export const NO_FILTERS: Filters = {
  testaments: [],
  genres: [],
  books: [],
  excludeTestaments: [],
  excludeGenres: [],
  excludeBooks: [],
  inside: [],
  overlaps: [],
  outside: [],
};

/** Filters resolved against the book data, ready to test passages quickly */
export interface CompiledFilter {
  /** Allowed book ids, or null for every book */
  books: Set<number> | null;
  inside: Passage[];
  overlaps: Passage[];
  outside: Passage[];
  /** Names or passages that could not be understood */
  errors: string[];
}

const TESTAMENT_BOOKS: Record<Testament, number[]> = {
  old: range(1, 39),
  new: range(40, 66),
};

function range(from: number, to: number): number[] {
  return Array.from({ length: to - from + 1 }, (_, i) => from + i);
}

export function isEmpty(filters: Filters): boolean {
  return Object.values(filters).every((values) => values.length === 0);
}

/**
 * Resolves names and passages, with the CLI's rules: inclusions of any kind are joined with
 * a logical OR and start from no books; exclusions apply after all inclusions.
 */
export function compileFilters(topos: Topos, filters: Filters): CompiledFilter {
  const errors: string[] = [];
  const bookIds = (names: string[]) =>
    names.flatMap((name) => {
      const id = topos.findBook(name);
      if (id === null) errors.push(`Unknown book "${name}"`);
      return id === null ? [] : [id];
    });
  const genreIds = (names: string[]) =>
    names.flatMap((name) => {
      const genre = topos.findGenre(name);
      if (genre === null) errors.push(`Unknown genre "${name}"`);
      // bookIds is a Uint8Array, which flatMap would not flatten
      return genre === null ? [] : Array.from(genre.bookIds);
    });
  const passages = (references: string[]) =>
    references.flatMap((reference) => {
      const passage = topos.parse(reference, 0);
      if (passage === null) errors.push(`Not a reference: "${reference}"`);
      return passage === null ? [] : [passage];
    });

  const included = [
    ...filters.testaments.flatMap((t) => TESTAMENT_BOOKS[t]),
    ...genreIds(filters.genres),
    ...bookIds(filters.books),
  ];
  const hasInclusion =
    filters.testaments.length + filters.genres.length + filters.books.length > 0;
  const excluded = new Set([
    ...filters.excludeTestaments.flatMap((t) => TESTAMENT_BOOKS[t]),
    ...genreIds(filters.excludeGenres),
    ...bookIds(filters.excludeBooks),
  ]);

  let books: Set<number> | null = null;
  if (hasInclusion || excluded.size > 0) {
    const start = hasInclusion ? included : topos.books().map((b) => b.id);
    books = new Set(start.filter((id) => !excluded.has(id)));
  }

  return {
    books,
    inside: passages(filters.inside),
    overlaps: passages(filters.overlaps),
    outside: passages(filters.outside),
    errors,
  };
}

/** Whether a found passage passes the filter */
export function keep(topos: Topos, filter: CompiledFilter, passage: Passage): boolean {
  if (filter.books && !filter.books.has(passage.bookId)) return false;
  const hasPassageInclusion = filter.inside.length + filter.overlaps.length > 0;
  if (
    hasPassageInclusion &&
    !filter.inside.some((outer) => topos.contains(outer, passage)) &&
    !filter.overlaps.some((other) => topos.overlaps(other, passage))
  ) {
    return false;
  }
  return !filter.outside.some((other) => topos.overlaps(other, passage));
}
