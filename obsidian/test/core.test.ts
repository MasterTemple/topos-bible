import assert from "node:assert/strict";
import { test } from "node:test";
import { BookStyle, Topos } from "topos-bible";
import { compileFilters, keep, NO_FILTERS, type Filters } from "../src/core/filters.ts";
import { linkUrl, siteName, SITES, templateError, templateFromTranslation } from "../src/core/links.ts";
import {
  applyReplacements,
  normalizeReferences,
  referenceAt,
  written,
} from "../src/core/references.ts";
import { ReferenceIndex, searchText } from "../src/core/search.ts";
import { groupHits, sortHits } from "../src/core/sort.ts";

const topos = Topos.new();
const parse = (reference: string) => topos.parse(reference, BookStyle.Name)!;

const books = new Map(topos.books().map((book) => [book.id, book]));
const link = (template: string, reference: string) => {
  const passage = parse(reference);
  return linkUrl(template, passage, books.get(passage.bookId));
};
const site = (id: string) => SITES.find((s) => s.id === id)!.template;

test("each site's links open the first verse or chapter", () => {
  const lw = site("literalword");
  assert.equal(link(lw, "John 3:16-18"), "https://app.literalword.com/43/3/16");
  assert.equal(link(lw, "Rom 8-9"), "https://app.literalword.com/45/8");
  assert.equal(link(templateFromTranslation("esv"), "Ps 23"), "https://app.literalword.com/esv/19/23");
  assert.equal(templateFromTranslation(""), lw);

  const hub = site("biblehub");
  assert.equal(link(hub, "1 Cor 13:4-7"), "https://biblehub.com/1_corinthians/13-4.htm");
  assert.equal(link(hub, "Ps 23"), "https://biblehub.com/psalms/23.htm");
  assert.equal(link(hub, "Song 2:1"), "https://biblehub.com/songs/2-1.htm");

  const gateway = site("biblegateway");
  assert.equal(
    link(gateway, "jn 3:16-18; 4"),
    "https://www.biblegateway.com/passage/?search=John%203%3A16-18%3B%204",
  );

  const youversion = site("youversion");
  assert.equal(link(youversion, "John 3:16-18"), "https://www.bible.com/bible/59/JHN.3.16-18");
  assert.equal(link(youversion, "1 John 4"), "https://www.bible.com/bible/59/1JN.4");
  // A range across chapters opens at its first verse
  assert.equal(link(youversion, "Phil 3:20-4:1"), "https://www.bible.com/bible/59/PHP.3.20");
});

test("link templates: placeholders, filters, and optional parts", () => {
  const template = "https://x.app/{book.abbreviation|lower|kebab}/{book.osis}/{chapter}[:{verse}][-{end_verse}][/to/{end_chapter}]?q={osis}";
  assert.equal(link(template, "1 Cor 13:4-7"), "https://x.app/1-cor/1Cor/13:4-7?q=1Cor.13.4-1Cor.13.7");
  assert.equal(link(template, "Gen 1-2"), "https://x.app/gn/Gen/1/to/2?q=Gen.1-Gen.2");
  assert.equal(link("https://x.app/{book|upper|compact}/{verse}", "John 3"), null);
  assert.equal(link("", "John 3"), null);
  assert.equal(link("https://x.app/{chapters}", "John 3"), null);

  assert.equal(templateError(site("youversion")), null);
  assert.equal(templateError("https://x.app/{chapters}"), "unknown placeholder {chapters}");
  assert.equal(templateError("{book|title}"), "unknown filter |title");
  assert.equal(templateError("{chapter}[/{verse}"), "a [ has no ]");
  assert.equal(templateError("[[{verse}]]"), "[ ... ] can't be nested");
  assert.equal(templateError("{chapter"), "a { has no }");

  assert.equal(siteName(site("biblehub")), "BibleHub");
  assert.equal(siteName("https://some-bible.app/v/{book.id}/{chapter}"), "some-bible.app");
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
  assert.deepEqual(filtered({ testaments: ["old"] }, text), ["Genesis 1:1", "Psalm 23"]);
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
  assert.deepEqual(filtered({ anyOverlap: ["Romans 8:28-9:1", "Ps 23:1"] }, text), [
    "Psalm 23",
    "Romans 8:28",
  ]);
  assert.deepEqual(filtered({ books: ["Romans"], excludeOverlap: ["Rom 9"] }, text), ["Romans 8:28"]);
});

test("testaments narrow genres and books, and contradictions are explained", () => {
  const text = "Gen 1:1, John 3:16, Rom 8:28, 2 Cor 4:4";
  assert.deepEqual(filtered({ testaments: ["new"], genres: ["Pauline Epistles"] }, text), [
    "Romans 8:28",
    "2 Corinthians 4:4",
  ]);
  assert.deepEqual(filtered({ testaments: ["old"], genres: ["Pauline Epistles"] }, text), []);
  const conflict = compileFilters(topos, { ...NO_FILTERS, testaments: ["old"], genres: ["Pauline Epistles"] });
  assert.equal(conflict.conflict, "Pauline Epistles is not in the Old Testament, so nothing can match");
  assert.equal(compileFilters(topos, { ...NO_FILTERS, testaments: ["new"], genres: ["Pauline Epistles"] }).conflict, null);
  // Exclusions always win, whatever else is included
  assert.deepEqual(filtered({ testaments: ["new"], excludeTestaments: ["new"] }, text), []);
});

test("passage filters that can't match anything are explained", () => {
  const conflict = (filters: Partial<Filters>) => compileFilters(topos, { ...NO_FILTERS, ...filters }).conflict;
  assert.equal(
    conflict({ books: ["Genesis"], inside: ["Romans 8"] }),
    "Romans 8 is in books that aren't searched, so nothing can match",
  );
  assert.equal(conflict({ testaments: ["new"], anyOverlap: ["Ps 23", "Gen 1"] }), "Ps 23, Gen 1 are in books that aren't searched, so nothing can match");
  assert.equal(conflict({ books: ["Genesis"], inside: ["Romans 8", "Gen 1"] }), null);
  assert.equal(
    conflict({ inside: ["John 3:16"], anyOverlap: ["John 3:1-5"], excludeOverlap: ["John 3"] }),
    "John 3:16, John 3:1-5 are within the excluded passages (John 3), so nothing can match",
  );
  assert.equal(conflict({ anyOverlap: ["John 3-4"], excludeOverlap: ["John 3"] }), null);
  // Genres and books add up, so a book outside the genres is not a conflict
  assert.equal(conflict({ genres: ["Pauline Epistles"], books: ["Genesis"] }), null);
});

test("explicit and exact overlap", () => {
  const text = "John 3, John 2-4, John 3:14-18, John 2; 3:16, Jn 3:16, John 3:16-17";
  assert.deepEqual(filtered({ anyOverlap: ["John 3:16"] }, text).length, 6);
  // Whole chapters in a reference don't count; John 2; 3:16 counts through 3:16
  assert.deepEqual(filtered({ explicitOverlap: ["John 3:16"] }, text), [
    "John 3:14-18",
    "John 2; 3:16",
    "John 3:16",
    "John 3:16-17",
  ]);
  // The passage given still covers its whole chapter
  assert.equal(filtered({ explicitOverlap: ["John 3"] }, text).length, 4);
  assert.deepEqual(filtered({ exactOverlap: ["John 3:16"] }, text), ["John 3:16"]);
  assert.deepEqual(filtered({ exactOverlap: ["John 3:1-36"] }, text), ["John 3"]);
  // Inclusions add up, exclusions win
  assert.deepEqual(
    filtered({ exactOverlap: ["John 3"], inside: ["John 3:14-17"], excludeOverlap: ["John 3:15"] }, text),
    ["John 3:16", "John 3:16-17"],
  );
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
  assert.equal(written(topos, passage, BookStyle.Name), "John 3:16-18");
  assert.equal(written(topos, passage, BookStyle.Abbreviation), "Jn 3:16-18");
  assert.equal(written(topos, passage, BookStyle.Osis), "John.3.16-John.3.18");

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

test("completions narrow by the number being typed", () => {
  const labels = (before: string, needsNumber = false) =>
    completionsBefore(topos, before, BookStyle.Name, 100, "capitalized", { needsNumber }).map(
      (c) => c.label,
    );
  // John has 21 chapters; what's already typed isn't offered (see complete.txt for the rest)
  assert.deepEqual(labels("John 2"), ["John 20", "John 21"]);
  assert.deepEqual(labels("John 3"), []);
  assert.deepEqual(labels("John 3:3").slice(0, 2), ["John 3:30", "John 3:31"]);
  assert.equal(labels("John 3:").length, 36);
  // In the editor, `John ` alone does not list chapters, but a number or delimiter does
  assert.deepEqual(labels("I asked John ", true), []);
  assert.deepEqual(labels("see John 2", true), ["John 20", "John 21"]);
  assert.equal(labels("see John 3:", true).length, 36);
  assert.equal(labels("see John 3:16, ", true)[0], "John 3:16,17");
  assert.ok(labels("see 1 Co", true).includes("1 Corinthians"));
});

test("applying a completion", () => {
  const text = "see gen 1:";
  const [first] = completionsBefore(topos, text, BookStyle.Name, 5, "capitalized");
  assert.equal(applyCompletion(text, first), "see Genesis 1:1");
});

test("reference inputs list every book, then add or complete what is typed", async () => {
  // inputs.tsx is JSX; its choice logic is plain TypeScript, so load it through esbuild
  const { referenceChoices } = await loadInputs();
  const describe = (value: string) =>
    referenceChoices(topos, BookStyle.Name, value, value.length).map((c: any) =>
      c.kind === "add" ? `add ${c.reference}` : c.completion.label,
    );
  const books = describe("");
  assert.equal(books.length, topos.books().length);
  assert.equal(books[0], "Genesis");
  // A finished reference is added first; completing it to itself is not offered
  assert.deepEqual(describe("Romans 8"), ["add Romans 8"]);
  assert.deepEqual(describe("John 3:16").slice(0, 1), ["add John 3:16"]);
  assert.equal(describe("John 3:16").includes("John 3:16"), false);
  // After a delimiter, the next numbers come first
  const range = describe("John 3:16-");
  assert.equal(range[0], "John 3:16-17");
  assert.equal(range.at(-1), "add John 3:16");
  // What's added is the formatted reference
  assert.deepEqual(describe("jn 3:16").slice(0, 1), ["add John 3:16"]);
  assert.deepEqual(describe("rom 8").slice(0, 1), ["add Romans 8"]);
  // Every verse of Psalm 119
  assert.equal(describe("Ps 119:").filter((c: string) => !c.startsWith("add")).length, 176);
});

async function loadInputs() {
  const esbuild = await import("esbuild");
  const { mkdirSync } = await import("node:fs");
  const out = new URL("../node_modules/.cache/inputs.mjs", import.meta.url).pathname;
  mkdirSync(new URL("../node_modules/.cache/", import.meta.url), { recursive: true });
  await esbuild.build({
    entryPoints: [new URL("../src/view/inputs.tsx", import.meta.url).pathname],
    bundle: true,
    format: "esm",
    platform: "node",
    jsx: "automatic",
    external: ["topos-bible", "react", "react-dom"],
    outfile: out,
    logLevel: "silent",
  });
  return import(out);
}
