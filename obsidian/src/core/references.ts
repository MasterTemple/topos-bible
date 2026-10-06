import { BookStyle, OffsetUnit, type Match, type Topos } from "topos-bible";

/** The reference written in a style: `John 3:16`, `Jn 3:16`, or `John.3.16` */
export function styled(
  topos: Topos,
  passage: { osis: string; reference: string },
  style: BookStyle,
): string {
  if (style === BookStyle.Name) return passage.reference;
  if (style === BookStyle.Osis) return passage.osis;
  return topos.parse(passage.osis, style)?.reference ?? passage.reference;
}

/** The reference in `line` that contains the column `ch` (UTF-16, 0-based), if any */
export function referenceAt(topos: Topos, line: string, ch: number): Match | null {
  return topos.search(line, OffsetUnit.Utf16).find((m) => m.start <= ch && ch <= m.end) ?? null;
}

/** An edit that rewrites every reference in `text` in a style (applied from the end backwards) */
export interface Replacement {
  start: number;
  end: number;
  text: string;
}

export function normalizeReferences(topos: Topos, text: string, style: BookStyle): Replacement[] {
  return topos
    .search(text, OffsetUnit.Utf16)
    .map((m) => ({ start: m.start, end: m.end, text: styled(topos, m.passage, style) }))
    .filter((r) => text.slice(r.start, r.end) !== r.text)
    .reverse();
}

/** Applies replacements (which must be sorted from the end of the text backwards) */
export function applyReplacements(text: string, replacements: Replacement[]): string {
  return replacements.reduce(
    (result, r) => result.slice(0, r.start) + r.text + result.slice(r.end),
    text,
  );
}
