import assert from "node:assert/strict";
import { test } from "node:test";
import { NO_FILTERS } from "../src/core/filters.ts";
import { formatQuery, parseQuery, tokenize } from "../src/core/query.ts";

test("queries are tokenized like a shell", () => {
  assert.deepEqual(tokenize(`--nt -g "Pauline Epistles" --book='Song of Solomon' -o John.3.16`), [
    "--nt",
    "-g",
    "Pauline Epistles",
    "--book=Song of Solomon",
    "-o",
    "John.3.16",
  ]);
});

test("queries use the CLI's filter options", () => {
  const { filters, folder, errors } = parseQuery(
    `Sermons/2025 --nt -t old --exclude-testament=new -g "Pauline Epistles" --exclude-genre gospels -b John --exclude-book Jude -i "Romans 8" -o "John 1" --outside "Ps 23"`,
  );
  assert.deepEqual(errors, []);
  assert.equal(folder, "Sermons/2025");
  assert.deepEqual(filters, {
    testaments: ["new", "old"],
    excludeTestaments: ["new"],
    genres: ["Pauline Epistles"],
    excludeGenres: ["gospels"],
    books: ["John"],
    excludeBooks: ["Jude"],
    inside: ["Romans 8"],
    overlaps: ["John 1"],
    outside: ["Ps 23"],
  });
});

test("query mistakes are reported", () => {
  assert.deepEqual(parseQuery("--nope -t middle a b -g").errors, [
    "Unknown option --nope",
    'Unknown testament "middle"',
    'Only one folder can be searched ("b")',
    "-g needs a value",
  ]);
  // The defaults are not changed by parsing
  assert.deepEqual(NO_FILTERS.genres, []);
});

test("formatting and parsing round-trip", () => {
  const filters = {
    ...NO_FILTERS,
    testaments: ["new" as const],
    genres: ["Pauline Epistles"],
    books: ["1 Cor"],
    overlaps: ["John 3:16-18"],
    outside: ['Ps "23"'],
  };
  const text = formatQuery(filters, "My Notes");
  assert.equal(text, `"My Notes" --nt -g "Pauline Epistles" -b "1 Cor" -o "John 3:16-18" --outside "Ps 23"`);
  const parsed = parseQuery(text);
  assert.deepEqual(parsed, { filters: { ...filters, outside: ["Ps 23"] }, folder: "My Notes", errors: [] });
  assert.equal(formatQuery(NO_FILTERS), "");
});
