import { CompletionKind, OffsetUnit, ToposFormat, type BookStyle, type Completion, type Topos } from "topos-bible";

export type BookCompletion = "off" | "capitalized" | "always";

/**
 * Completions for the reference that ends `before` (the text before the cursor). The engine
 * decides what to complete, exactly as in the CLI and the language server (the shared cases in
 * crates/topos-lib/tests/cases/complete.txt check all of them); this only adds the editor's
 * typing preferences:
 * - `books`: whether book names complete at all, or only for words starting with a capital
 *   letter or a number (`capitalized`), so ordinary prose isn't interrupted
 * - `needsNumber`: in prose, `John ` alone doesn't list chapters until a number or a delimiter
 *   follows (`John 3`, `John 3:`, `John 3:16, `)
 * - `joinAdjacent`: write adjacent verses as a range (`3:16-18`, not `3:16,17,18`)
 */
export function completionsBefore(
  topos: Topos,
  before: string,
  style: BookStyle,
  limit: number,
  books: BookCompletion,
  { needsNumber = false, joinAdjacent = false }: { needsNumber?: boolean; joinAdjacent?: boolean } = {},
): Completion[] {
  const format = ToposFormat.create().book(style).joinAdjacent(joinAdjacent);
  let completions: Completion[];
  try {
    completions = topos.completeWith(before, before.length, OffsetUnit.Utf16, format, 0);
  } finally {
    format.dispose();
  }
  const waiting = needsNumber && !/[\d:.,;\-–—]\s*$/.test(before);
  return completions
    .filter((c) => {
      if (c.kind !== CompletionKind.Book) return !waiting;
      if (books === "off") return false;
      const typed = before.slice(c.start, c.end).trim();
      return books !== "capitalized" || /^[\p{Lu}\d]/u.test(typed);
    })
    .slice(0, limit || undefined);
}

/** The text after applying a completion */
export function applyCompletion(text: string, completion: Completion): string {
  return text.slice(0, completion.start) + completion.text + text.slice(completion.end);
}
