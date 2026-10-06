import type { Passage, Topos } from "topos-bible";

export type Testament = "old" | "new";

/** The search filters, like the CLI's options */
export interface Filters {
  /**
   * Included testaments limit the search; included genres and books add up within them;
   * exclusions always win (the CLI's rules)
   */
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
  /** Why nothing can match, when the included genres and books are outside the testaments */
  conflict: string | null;
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

/** Resolves names and passages, with the CLI's rules (see {@link Filters}) */
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

  const scope =
    filters.testaments.length > 0 ? new Set(filters.testaments.flatMap((t) => TESTAMENT_BOOKS[t])) : null;
  const included =
    filters.genres.length + filters.books.length > 0
      ? new Set([...genreIds(filters.genres), ...bookIds(filters.books)])
      : null;
  const excluded = new Set([
    ...filters.excludeTestaments.flatMap((t) => TESTAMENT_BOOKS[t]),
    ...genreIds(filters.excludeGenres),
    ...bookIds(filters.excludeBooks),
  ]);

  let books: Set<number> | null = null;
  if (scope || included || excluded.size > 0) {
    books = new Set(
      topos
        .books()
        .map((b) => b.id)
        .filter((id) => (!scope || scope.has(id)) && (!included || included.has(id)) && !excluded.has(id)),
    );
  }

  let conflict: string | null = null;
  if (scope && included && included.size > 0 && ![...included].some((id) => scope.has(id))) {
    const testaments = filters.testaments.map((t) => (t === "old" ? "Old" : "New")).join(" and ");
    const names = [...filters.genres, ...filters.books].join(", ");
    conflict = `${names} ${filters.genres.length + filters.books.length === 1 ? "is" : "are"} not in the ${testaments} Testament${filters.testaments.length === 1 ? "" : "s"}, so nothing can match`;
  }

  return {
    books,
    conflict,
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
