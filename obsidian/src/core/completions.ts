import { CompletionKind, OffsetUnit, type BookStyle, type Completion, type Topos } from "topos-bible";

export type BookCompletion = "off" | "capitalized" | "always";

/**
 * Completions for the reference that ends `before` (the text before the cursor), filtered so
 * book names don't pop up for ordinary words:
 * - a book is offered when the typed word starts its name, abbreviation, or OSIS id (`Jo` for
 *   John), or is itself an abbreviation (`Jn`), but not for `The` (from "the revelation")
 * - in `capitalized` mode the word must start with a capital letter or a number
 */
export function completionsBefore(
  topos: Topos,
  before: string,
  style: BookStyle,
  limit: number,
  books: BookCompletion,
  { needsNumber = false }: { needsNumber?: boolean } = {},
): Completion[] {
  // Narrow by the number being typed: `John 3` offers 3 and 30-39, not every chapter. The
  // engine's suggestions ignore it, so ask for all of them and filter here.
  const typedNumber = /(\d+)$/.exec(before)?.[1] ?? "";
  // In prose, `John ` alone should not open a list of chapters: wait for a number or a
  // delimiter (`John 3`, `John 3:`, `John 3:16, `)
  if (needsNumber && !/[\d:.,;\-–—]\s*$/.test(before)) {
    return completeBooksOnly(topos, before, style, limit, books);
  }
  const completions = topos.complete(before, before.length, OffsetUnit.Utf16, style, 0);
  return completions
    .filter((c) => c.kind === CompletionKind.Book || endsWithNumber(c.text, typedNumber))
    .filter((c) => {
    if (c.kind !== CompletionKind.Book) return true;
    if (books === "off") return false;
    const typed = before.slice(c.start, c.end).trim();
    if (books === "capitalized" && !/^[\p{Lu}\d]/u.test(typed)) return false;
    return startsBook(topos, typed, c.label);
  })
    .slice(0, limit || undefined);
}

/** Only book-name completions (for the start of a reference in prose) */
function completeBooksOnly(
  topos: Topos,
  before: string,
  style: BookStyle,
  limit: number,
  books: BookCompletion,
): Completion[] {
  return completionsBefore(topos, before, style, limit, books).filter(
    (c) => c.kind === CompletionKind.Book,
  );
}

/** Whether the last number in `text` starts with `typed` (`John 3:16` for `1` or `16`) */
function endsWithNumber(text: string, typed: string): boolean {
  if (!typed) return true;
  const last = /(\d+)\D*$/.exec(text)?.[1] ?? "";
  return last.startsWith(typed);
}

function startsBook(topos: Topos, typed: string, label: string): boolean {
  const lower = typed.toLowerCase();
  if (topos.findBook(typed) !== null) return true;
  const id = topos.findBook(label);
  const book = topos.books().find((b) => b.id === id);
  return [book?.name, book?.abbreviation, book?.osis, label].some((name) =>
    name?.toLowerCase().startsWith(lower),
  );
}

/** The text after applying a completion */
export function applyCompletion(text: string, completion: Completion): string {
  return text.slice(0, completion.start) + completion.text + text.slice(completion.end);
}
