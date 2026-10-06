import type { Passage } from "topos-bible";

/** Translations Literal Word accepts in its links ("" uses the app's default) */
export const TRANSLATIONS = ["", "nasb", "lsb", "esv", "nkjv", "kjv"] as const;
export type Translation = (typeof TRANSLATIONS)[number];

/**
 * A Literal Word deep link (https://app.literalword.com/deep-links) for a passage.
 *
 * Literal Word opens one verse or one chapter, so a passage opens at its first verse, or at
 * its first chapter when it starts with whole chapters. Books are numbered 1-66, so passages
 * in other books (from custom data) have no link.
 */
export function literalWordUrl(passage: Passage, translation: Translation = ""): string | null {
  const first = passage.segments[0];
  if (!first || passage.bookId < 1 || passage.bookId > 66) return null;
  const location =
    first.tag === "Verses"
      ? `${first.start.chapter}/${first.start.verse}`
      : `${first.start}`;
  const prefix = translation ? `${translation}/` : "";
  return `https://app.literalword.com/${prefix}${passage.bookId}/${location}`;
}
