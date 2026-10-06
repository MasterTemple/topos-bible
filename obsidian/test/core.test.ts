import assert from "node:assert/strict";
import { test } from "node:test";
import { BookStyle, Topos } from "topos-bible";
import { compileFilters, keep, NO_FILTERS, type Filters } from "../src/core/filters.ts";
import { literalWordUrl } from "../src/core/literalWord.ts";
import {
  applyReplacements,
  normalizeReferences,
  referenceAt,
  styled,
} from "../src/core/references.ts";
import { ReferenceIndex, searchText } from "../src/core/search.ts";
import { groupHits, sortHits } from "../src/core/sort.ts";

const topos = Topos.new();
const parse = (reference: string) => topos.parse(reference, BookStyle.Name)!;

test("Literal Word links open the first verse or chapter", () => {
  assert.equal(literalWordUrl(parse("John 3:16-18")), "https://app.literalword.com/43/3/16");
  assert.equal(literalWordUrl(parse("Ps 23"), "esv"), "https://app.literalword.com/esv/19/23");
  assert.equal(literalWordUrl(parse("Rom 8-9")), "https://app.literalword.com/45/8");
});

test("search hits carry UTF-16 positions and their line", () => {
  const text = "intro\n📖 Read Jn 3:16 today\nand Rom 8:28";
  const hits = searchText(topos, "a.md", text);
  assert.deepEqual(
    hits.map((h) => [h.passage.reference, text.slice(h.start, h.end), h.line, h.column, h.lineText]),
    [
      ["John 3:16", "Jn 3:16", 2, 9, "📖 Read Jn 3:16 today"],
      ["Romans 8:28", "Rom 8:28", 3, 5, "and Rom 8:28"],
    ],
  );
});

function filtered(filters: Partial<Filters>, text: string): string[] {
  const filter = compileFilters(topos, { ...NO_FILTERS, ...filters });
  assert.deepEqual(filter.errors, []);
  return searchText(topos, "a.md", text)
    .filter((h) => keep(topos, filter, h.passage))
    .map((h) => h.passage.reference);
}

test("filters follow the CLI's rules", () => {
  const text = "Gen 1:1, Ps 23, Matt 5:3, John 3:16, Rom 8:28, Rom 9:2, Jude 5";
  assert.deepEqual(filtered({ testaments: ["old"] }, text), ["Genesis 1:1", "Psalms 23"]);
  assert.deepEqual(filtered({ genres: ["gospels"], books: ["Jude"] }, text), [
    "Matthew 5:3",
    "John 3:16",
    "Jude 1:5",
  ]);
  assert.deepEqual(filtered({ testaments: ["new"], excludeGenres: ["pauline"] }, text), [
    "Matthew 5:3",
    "John 3:16",
    "Jude 1:5",
  ]);
  assert.deepEqual(filtered({ inside: ["Romans 8"] }, text), ["Romans 8:28"]);
  assert.deepEqual(filtered({ overlaps: ["Romans 8:28-9:1", "Ps 23:1"] }, text), [
    "Psalms 23",
    "Romans 8:28",
  ]);
  assert.deepEqual(filtered({ books: ["Romans"], outside: ["Rom 9"] }, text), ["Romans 8:28"]);
});

test("unknown names are reported, not silently dropped", () => {
  const filter = compileFilters(topos, { ...NO_FILTERS, books: ["Jhon"], inside: ["nope"] });
  assert.deepEqual(filter.errors, ['Unknown book "Jhon"', 'Not a reference: "nope"']);
});

test("the index follows file changes", () => {
  const index = new ReferenceIndex(topos);
  index.update("b.md", "Rom 8:28");
  index.update("a.md", "John 3:16 and Gen 1:1");
  index.rename("b.md", "c.md");
  assert.deepEqual(
    index.all().map((h) => `${h.path} ${h.passage.reference}`),
    ["a.md John 3:16", "a.md Genesis 1:1", "c.md Romans 8:28"],
  );
  index.remove("a.md");
  assert.equal(index.all().length, 1);
});

test("sorting and grouping", () => {
  const hits = [
    ...searchText(topos, "b.md", "John 3:16, Gen 1:1"),
    ...searchText(topos, "a.md", "Gen 2:1, John 1:1"),
  ];
  const bible = sortHits(hits, "bible").map((h) => `${h.passage.reference} ${h.path}`);
  assert.deepEqual(bible, ["Genesis 1:1 b.md", "Genesis 2:1 a.md", "John 1:1 a.md", "John 3:16 b.md"]);
  const groups = groupHits(sortHits(hits, "file"), "book", (id) => (id === 1 ? "Genesis" : "John"));
  assert.deepEqual(
    groups.map((g) => [g.key, g.hits.length]),
    [["Genesis", 2], ["John", 2]],
  );
});

test("references in a style, under the cursor, and normalized", () => {
  const passage = parse("jn 3:16-18");
  assert.equal(styled(topos, passage, BookStyle.Name), "John 3:16-18");
  assert.equal(styled(topos, passage, BookStyle.Abbreviation), "Jn 3:16-18");
  assert.equal(styled(topos, passage, BookStyle.Osis), "John.3.16-John.3.18");

  const line = "See jn 3:16 and Rom 8:28";
  assert.equal(referenceAt(topos, line, 6)?.passage.reference, "John 3:16");
  assert.equal(referenceAt(topos, line, 13), null);

  const text = "See jn 3:16 and 1 cor 13:4-7.";
  const replacements = normalizeReferences(topos, text, BookStyle.Name);
  assert.equal(applyReplacements(text, replacements), "See John 3:16 and 1 Corinthians 13:4-7.");
});

import { applyCompletion, completionsBefore } from "../src/core/completions.ts";

test("completions skip book names for ordinary words", () => {
  const labels = (before: string, mode: "off" | "capitalized" | "always" = "capitalized") =>
    completionsBefore(topos, before, BookStyle.Name, 50, mode).map((c) => c.label);
  assert.ok(labels("I like Jo").includes("John"));
  assert.ok(labels("see Jn").includes("John"));
  assert.deepEqual(labels("The"), []);
  assert.deepEqual(labels("First"), []);
  assert.deepEqual(labels("I like jo"), []);
  assert.ok(labels("I like jo", "always").includes("John"));
  assert.deepEqual(labels("Jo", "off"), []);
  // Chapters and verses always complete
  assert.equal(labels("see John 3:")[0], "John 3:1");
  assert.equal(labels("see 1 Co")[0], "1 Corinthians");
});

test("applying a completion", () => {
  const text = "see gen 1:";
  const [first] = completionsBefore(topos, text, BookStyle.Name, 5, "capitalized");
  assert.equal(applyCompletion(text, first), "see Genesis 1:1");
});
