import assert from "node:assert/strict";
import { mkdtempSync, mkdirSync, writeFileSync, existsSync } from "node:fs";
import { createRequire } from "node:module";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { test } from "node:test";
import { Topos } from "topos-bible";
import { cliFailed, CliOutputParser, OutdatedCliError, parseCliLine } from "../src/core/cli.ts";
import { cliBatches } from "../src/core/epub.ts";
import { searchText, type Hit } from "../src/core/search.ts";
import { runCli } from "../src/indexers/cli.ts";
import { handle } from "../src/indexers/worker.ts";

const topos = Topos.new();
// The plugin loads Node's modules with require (desktop only), as Obsidian's CommonJS bundle allows
(globalThis as { require?: NodeJS.Require }).require ??= createRequire(import.meta.url);

const line = (path: string, reference: string, start: number) =>
  JSON.stringify({
    path,
    reference,
    osis: "John.3.16",
    book_id: 43,
    book: "John",
    segments: [{ type: "verse", chapter: 3, verse: 16 }],
    line: 1,
    utf16_column: start + 1,
    start_utf16: start,
    end_utf16: start + 7,
    line_text: "Jn 3:16",
  });

test("CLI JSON lines become hits with vault paths", () => {
  const hit = parseCliLine(line("./notes\\a.md", "John 3:16", 0))!;
  assert.equal(hit.path, "notes/a.md");
  assert.equal(hit.passage.bookId, 43);
  assert.equal(hit.passage.osis, "John.3.16");
  assert.deepEqual([hit.start, hit.end, hit.column, hit.lineText], [0, 7, 1, "Jn 3:16"]);
  assert.equal(parseCliLine("  "), null);
  assert.throws(() => parseCliLine(JSON.stringify({ path: "a.md", reference: "John 3:16" })), OutdatedCliError);
});

test("CLI JSON for EPUBs says where in the book a reference is", () => {
  const json = JSON.parse(line("Books/b.epub", "John 3:16", 40));
  json.epub = { spine_index: 3, cfi: "epubcfi(/6/8!/4/2,/1:0,/1:7)", chapter: "One" };
  const hit = parseCliLine(JSON.stringify(json))!;
  assert.deepEqual(hit.epub, { spineIndex: 3, cfi: "epubcfi(/6/8!/4/2,/1:0,/1:7)", chapter: "One" });
  assert.deepEqual([hit.start, hit.end], [40, 47]);
  assert.equal(parseCliLine(line("a.md", "John 3:16", 0))!.epub, undefined);
});

test("files the CLI couldn't read are found in its errors", () => {
  const errors = ["topos: Books/a b.epub: invalid zip", "topos: .\\Books\\c.epub: bad"];
  assert.ok(cliFailed(errors, "Books/a b.epub"));
  assert.ok(cliFailed(errors, "Books/c.epub"));
  assert.ok(!cliFailed(errors, "Books/a.epub"));
});

test("paths for the CLI are split into batches that fit on a command line", () => {
  assert.deepEqual(cliBatches(["aaaa", "bb", "cccc", "d"], 12), [["aaaa", "bb"], ["cccc", "d"]]);
  assert.deepEqual(cliBatches(["a-very-long-path"], 10), [["a-very-long-path"]]);
  assert.deepEqual(cliBatches([]), []);
});

test("streamed CLI output is grouped by file, across chunk boundaries", () => {
  const files: [string, number][] = [];
  const parser = new CliOutputParser((path, hits) => files.push([path, hits.length]));
  const output = [line("a.md", "John 3:16", 0), line("a.md", "John 3:16", 9), line("b.md", "John 3:16", 0)].join("\n") + "\n";
  for (let i = 0; i < output.length; i += 17) parser.push(output.slice(i, i + 17));
  parser.finish();
  assert.deepEqual(files, [
    ["a.md", 2],
    ["b.md", 1],
  ]);
});

test("the worker searches files like the main thread", async () => {
  const files = [
    { path: "a.md", text: "Read Jn 3:16 and Rom 8:28" },
    { path: "b.md", text: "nothing here" },
  ];
  const response = await handle({ type: "search", id: 7, files }, topos);
  assert.equal(response.type, "results");
  if (response.type !== "results") return;
  assert.equal(response.id, 7);
  assert.deepEqual(response.results[0].hits, searchText(topos, "a.md", files[0].text));
  assert.deepEqual(response.results[1].hits, []);
  // Results cross to the main thread by structured clone
  assert.deepEqual(structuredClone(response), response);
});

const cli = join(import.meta.dirname, "../../target/debug/topos");

test("the CLI and the built-in engine find the same references", { skip: !existsSync(cli) && "build topos-bible-cli first" }, async () => {
  const vault = mkdtempSync(join(tmpdir(), "topos-vault-"));
  mkdirSync(join(vault, "sub"));
  const notes: Record<string, string> = {
    "a.md": "é 📖 Jn 3:16-18; 5\nnext Rom 8:28",
    "sub/b.md": "# Psalms\nPs 23 and 1 Cor 13:4-7, 13",
    "c.md": "no references",
  };
  for (const [path, text] of Object.entries(notes)) writeFileSync(join(vault, path), text);
  writeFileSync(join(vault, "skipped.html"), "<p>Gen 1:1</p>");

  const found = new Map<string, Hit[]>();
  const run = runCli(cli, vault, { cache: false, extensions: ["md"] }, (path, hits) => found.set(path, hits));
  await run.done;

  assert.deepEqual([...found.keys()].sort(), ["a.md", "sub/b.md"]);
  for (const [path, hits] of found) {
    const expected = searchText(topos, path, notes[path]);
    const simplify = (h: Hit) => [h.start, h.end, h.line, h.column, h.lineText, h.passage.reference, h.passage.osis, h.passage.segments];
    assert.deepEqual(hits.map(simplify), expected.map(simplify), path);
  }
});

test("the CLI searches named files, and reports those it can't read", { skip: !existsSync(cli) && "build topos-bible-cli first" }, async () => {
  const vault = mkdtempSync(join(tmpdir(), "topos-named-"));
  writeFileSync(join(vault, "-dash.md"), "Jn 3:16");
  writeFileSync(join(vault, "broken.epub"), "not a zip");
  const found = new Map<string, Hit[]>();
  const run = runCli(cli, vault, { cache: false, extensions: [], paths: ["-dash.md", "broken.epub"] }, (path, hits) => found.set(path, hits));
  await run.done;
  assert.deepEqual([...found.keys()], ["-dash.md"]);
  assert.ok(cliFailed(run.errors, "broken.epub"), run.errors.join("\n"));
  assert.ok(!cliFailed(run.errors, "-dash.md"));
});

test("a topos that rejects the arguments is an error, not an empty vault", { skip: process.platform === "win32" }, async () => {
  const dir = mkdtempSync(join(tmpdir(), "topos-old-"));
  const old = join(dir, "topos");
  writeFileSync(old, "#!/bin/sh\necho \"error: unexpected argument '--ext' found\" >&2\nexit 2\n", { mode: 0o755 });
  const run = runCli(old, dir, { cache: false, extensions: ["md"] }, () => {});
  await assert.rejects(run.done, /unexpected argument/);
});
