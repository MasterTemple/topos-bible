import type { EpubReference } from "topos-bible";
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

/** Hits from EPUB++'s text (with CFIs) as the engine's references, for `ToposIndex.setEpub` */
export function epubReferences(hits: Hit[]): EpubReference[] {
  return hits.flatMap((hit) =>
    hit.epub
      ? [
          {
            passage: hit.passage,
            start: hit.start,
            end: hit.end,
            line: hit.line,
            column: hit.column,
            spineIndex: hit.epub.spineIndex,
            chapter: hit.epub.chapter,
            cfi: hit.epub.cfi,
            lineText: hit.lineText,
          },
        ]
      : [],
  );
}
