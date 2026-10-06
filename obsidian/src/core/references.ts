import { OffsetUnit, type BookStyle, type Match, type Topos } from "topos-bible";
import { DEFAULT_FORMAT, written, type FormatSettings } from "./format.ts";

export { written };

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

export function normalizeReferences(
  topos: Topos,
  text: string,
  style: BookStyle,
  format: FormatSettings = DEFAULT_FORMAT,
): Replacement[] {
  return topos
    .search(text, OffsetUnit.Utf16)
    .map((m) => ({ start: m.start, end: m.end, text: written(topos, m.passage, style, format) }))
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
