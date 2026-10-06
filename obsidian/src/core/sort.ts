import type { Passage } from "topos-bible";
import type { Hit } from "./search.ts";

export type SortOrder = "file" | "bible";

/** Where a passage starts in the Bible, for sorting: [book, chapter, verse] */
export function bibleOrder(passage: Passage): [number, number, number] {
  const first = passage.segments[0];
  if (!first) return [passage.bookId, 0, 0];
  return first.tag === "Verses"
    ? [passage.bookId, first.start.chapter, first.start.verse]
    : [passage.bookId, first.start, 0];
}

function compareTuples(a: number[], b: number[]): number {
  for (let i = 0; i < Math.min(a.length, b.length); i++) {
    if (a[i] !== b[i]) return a[i] - b[i];
  }
  return a.length - b.length;
}

export function sortHits(hits: Hit[], order: SortOrder): Hit[] {
  const byFile = (a: Hit, b: Hit) => a.path.localeCompare(b.path) || a.start - b.start;
  return [...hits].sort(
    order === "file"
      ? byFile
      : (a, b) => compareTuples(bibleOrder(a.passage), bibleOrder(b.passage)) || byFile(a, b),
  );
}

export interface Group {
  key: string;
  hits: Hit[];
}

/** Groups sorted hits by file, or by book (named with `bookName`) */
export function groupHits(
  hits: Hit[],
  by: "file" | "book",
  bookName: (id: number) => string,
): Group[] {
  const groups = new Map<string, Hit[]>();
  for (const hit of hits) {
    const key = by === "file" ? hit.path : bookName(hit.passage.bookId);
    const group = groups.get(key);
    if (group) group.push(hit);
    else groups.set(key, [hit]);
  }
  return [...groups].map(([key, hits]) => ({ key, hits }));
}
