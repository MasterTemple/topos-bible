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
): Completion[] {
  const completions = topos.complete(before, before.length, OffsetUnit.Utf16, style, limit);
  return completions.filter((c) => {
    if (c.kind !== CompletionKind.Book) return true;
    if (books === "off") return false;
    const typed = before.slice(c.start, c.end).trim();
    if (books === "capitalized" && !/^[\p{Lu}\d]/u.test(typed)) return false;
    return startsBook(topos, typed, c.label);
  });
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
