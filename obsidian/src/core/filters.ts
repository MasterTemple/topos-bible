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
  /** Keep references entirely inside one of these passages (`-i`) */
  inside: string[];
  /** Keep references that share any verse with one of these passages (`--any-overlap`) */
  anyOverlap: string[];
  /**
   * Keep references that name a verse of one of these passages: whole chapters in a reference
   * don't count (`-o`)
   */
  explicitOverlap: string[];
  /** Keep references that are exactly one of these passages (`--exact-overlap`) */
  exactOverlap: string[];
  /** Drop references that share any verse with one of these passages (`--exclude-overlap`) */
  excludeOverlap: string[];
}

/** The filters that keep references by passage (joined with OR) */
export const PASSAGE_INCLUSIONS = ["inside", "anyOverlap", "explicitOverlap", "exactOverlap"] as const;

export const NO_FILTERS: Filters = {
  testaments: [],
  genres: [],
  books: [],
  excludeTestaments: [],
  excludeGenres: [],
  excludeBooks: [],
  inside: [],
  anyOverlap: [],
  explicitOverlap: [],
  exactOverlap: [],
  excludeOverlap: [],
};

/** Filters resolved against the book data, ready to test passages quickly */
export interface CompiledFilter {
  /** Allowed book ids, or null for every book */
  books: Set<number> | null;
  /** Why nothing can match, when the included genres and books are outside the testaments */
  conflict: string | null;
  inside: Passage[];
  anyOverlap: Passage[];
  explicitOverlap: Passage[];
  exactOverlap: Passage[];
  excludeOverlap: Passage[];
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

  const inside = passages(filters.inside);
  const anyOverlap = passages(filters.anyOverlap);
  const explicitOverlap = passages(filters.explicitOverlap);
  const exactOverlap = passages(filters.exactOverlap);
  const excludeOverlap = passages(filters.excludeOverlap);

  let conflict: string | null = null;
  const list = (names: string[]) => `${names.join(", ")} ${names.length === 1 ? "is" : "are"}`;
  if (scope && included && included.size > 0 && ![...included].some((id) => scope.has(id))) {
    const testaments = filters.testaments.map((t) => (t === "old" ? "Old" : "New")).join(" and ");
    conflict = `${list([...filters.genres, ...filters.books])} not in the ${testaments} Testament${filters.testaments.length === 1 ? "" : "s"}, so nothing can match`;
  } else {
    // The passages that keep references, with what was typed (unparsed ones are already errors)
    const named = PASSAGE_INCLUSIONS.flatMap((key) => filters[key]).flatMap((text) => {
      const passage = topos.parse(text, 0);
      return passage ? [{ text, passage }] : [];
    });
    const searched = named.filter(({ passage }) => !books || books.has(passage.bookId));
    if (named.length > 0 && searched.length === 0) {
      conflict = `${list(named.map((p) => p.text))} in books that aren't searched, so nothing can match`;
    } else if (
      searched.length > 0 &&
      searched.every(({ passage }) => excludeOverlap.some((o) => topos.contains(o, passage)))
    ) {
      conflict = `${list(searched.map((p) => p.text))} within the excluded passages (${filters.excludeOverlap.join(", ")}), so nothing can match`;
    }
  }

  return { books, conflict, inside, anyOverlap, explicitOverlap, exactOverlap, excludeOverlap, errors };
}

/** The parts of a passage that name verses, leaving out whole chapters, or null if none do */
export function explicitVerses(passage: Passage): Passage | null {
  const segments = passage.segments.filter((segment) => segment.tag === "Verses");
  return segments.length > 0 ? { ...passage, segments } : null;
}

/** Whether a found passage passes the filter */
export function keep(topos: Topos, filter: CompiledFilter, passage: Passage): boolean {
  if (filter.books && !filter.books.has(passage.bookId)) return false;
  const hasPassageInclusion = PASSAGE_INCLUSIONS.some((key) => filter[key].length > 0);
  if (hasPassageInclusion) {
    const named = filter.explicitOverlap.length > 0 ? explicitVerses(passage) : null;
    const included =
      filter.inside.some((outer) => topos.contains(outer, passage)) ||
      filter.anyOverlap.some((other) => topos.overlaps(other, passage)) ||
      (named !== null && filter.explicitOverlap.some((other) => topos.overlaps(other, named))) ||
      filter.exactOverlap.some((other) => topos.contains(other, passage) && topos.contains(passage, other));
    if (!included) return false;
  }
  return !filter.excludeOverlap.some((other) => topos.overlaps(other, passage));
}
