import assert from "node:assert/strict";
import { mkdtempSync, mkdirSync, writeFileSync, existsSync } from "node:fs";
import { createRequire } from "node:module";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { test } from "node:test";
import { OffsetUnit, Topos } from "topos-bible";
import { cliBatches, cliFailed, CliOutputParser, OutdatedCliError, parseIndexLine, vaultPath } from "../src/core/cli.ts";
import { ReferenceIndex } from "../src/core/index.ts";
import { searchText, type Hit } from "../src/core/search.ts";
import { runCli } from "../src/indexers/cli.ts";
import { handle } from "../src/indexers/worker.ts";
import { epub } from "./epub.ts";

const topos = Topos.new();
// The plugin loads Node's modules with require (desktop only), as Obsidian's CommonJS bundle allows
(globalThis as { require?: NodeJS.Require }).require ??= createRequire(import.meta.url);

const simplify = (h: Hit) => [h.path, h.start, h.end, h.line, h.column, h.passage.reference, h.passage.osis, h.passage.segments];

/** A `topos -m index` line for a text, made by the engine */
const line = (path: string, text: string) =>
  JSON.stringify({
    path,
    entry: Buffer.from(topos.indexEntry(path, text.length, 1, text, 1, OffsetUnit.Utf16)).toString("base64"),
  });

test("CLI index lines become entries with vault paths", () => {
  const entry = parseIndexLine(line("./notes\\a.md", "Jn 3:16"))!;
  assert.equal(entry.path, "notes/a.md");
  const index = new ReferenceIndex(topos);
  index.insert(entry.bytes, entry.path, { size: 7, mtime: 1 });
  assert.deepEqual(index.get("notes/a.md").map(simplify), searchText(topos, "notes/a.md", "Jn 3:16").map((h) => simplify({ ...h, lineText: "" })));
  assert.equal(parseIndexLine("  "), null);
  assert.equal(vaultPath(".\\a\\b.md"), "a/b.md");
});

test("streamed CLI output is split into lines, across chunk boundaries", () => {
  const paths: string[] = [];
  const parser = new CliOutputParser((path) => paths.push(path));
  const output = [line("a.md", "John 3:16"), line("b.md", ""), line("c.md", "Gen 1:1")].join("\n") + "\n";
  for (let i = 0; i < output.length; i += 17) parser.push(output.slice(i, i + 17));
  parser.finish();
  assert.deepEqual(paths, ["a.md", "b.md", "c.md"]);
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

test("the worker searches files into index entries, like the main thread", async () => {
  const files = [
    { path: "a.md", size: 25, mtime: 3, text: "Read Jn 3:16 and Rom 8:28" },
    { path: "b.md", size: 12, mtime: 3, text: "nothing here" },
  ];
  const response = await handle({ type: "index", id: 7, files }, topos);
  assert.equal(response.type, "entries");
  if (response.type !== "entries") return;
  assert.equal(response.id, 7);
  // Entries cross to the main thread by structured clone (their buffers are transferred)
  const entries = structuredClone(response).entries;
  const fromWorker = new ReferenceIndex(topos);
  const here = new ReferenceIndex(topos);
  for (const [i, { path, bytes }] of entries.entries()) {
    fromWorker.insert(bytes, path, files[i]);
    here.update(path, files[i].text, files[i]);
  }
  assert.deepEqual(fromWorker.all().map(simplify), here.all().map(simplify));
  assert.equal(fromWorker.all().length, 2);
  // EPUB++'s sections are still searched into hits
  const search = await handle({ type: "search", id: 8, files: [files[0]] }, topos);
  assert.equal(search.type === "results" && search.results[0].hits.length, 2);
});

const cli = join(import.meta.dirname, "../../target/debug/topos");

test("the worker searches EPUBs like the CLI", { skip: !existsSync(cli) && "build topos-bible-cli first" }, async () => {
  const bytes = epub(["See Jn 3:16.", "And Romans 8:28 and Ps 23"]);
  const book = { path: "Books/b.epub", size: bytes.length, mtime: 5, bytes: bytes.slice().buffer };
  const response = await handle({ type: "book", id: 1, book }, topos);
  assert.ok(response.type === "book" && "bytes" in response.result, JSON.stringify(response));
  const fromWorker = new ReferenceIndex(topos);
  fromWorker.insert(response.result.bytes, "Books/b.epub", book);

  const dir = mkdtempSync(join(tmpdir(), "topos-epub-"));
  mkdirSync(join(dir, "Books"));
  writeFileSync(join(dir, "Books/b.epub"), bytes);
  const fromCli = new ReferenceIndex(topos);
  await runCli(cli, dir, { cache: false, paths: ["Books/b.epub"] }, (path, entry) => fromCli.insert(entry, path, book)).done;

  const all = (index: ReferenceIndex) => index.all().map((h) => [...simplify(h), h.lineText, h.epub]);
  assert.deepEqual(all(fromWorker), all(fromCli));
  const hits = fromWorker.get("Books/b.epub");
  assert.deepEqual(
    hits.map((h) => [h.passage.reference, h.epub?.chapter, h.epub?.cfi, h.lineText]),
    [
      ["John 3:16", "Chapter 1", "epubcfi(/6/2!/4/2,/1:4,/1:11)", "See Jn 3:16."],
      ["Romans 8:28", "Chapter 2", "epubcfi(/6/4!/4/2,/1:4,/1:15)", "And Romans 8:28 and Ps 23"],
      ["Psalm 23", "Chapter 2", "epubcfi(/6/4!/4/2,/1:20,/1:25)", "And Romans 8:28 and Ps 23"],
    ],
  );
  // A file that isn't an EPUB says why
  const broken = await handle({ type: "book", id: 2, book: { ...book, bytes: new ArrayBuffer(8) } }, topos);
  assert.ok(broken.type === "book" && "error" in broken.result && /zip|EPUB/i.test(broken.result.error), JSON.stringify(broken));
});

test("the CLI and the built-in engine index the same references", { skip: !existsSync(cli) && "build topos-bible-cli first" }, async () => {
  const vault = mkdtempSync(join(tmpdir(), "topos-vault-"));
  mkdirSync(join(vault, "sub"));
  const notes: Record<string, string> = {
    "a.md": "é 📖 Jn 3:16-18; 5\nnext Rom 8:28",
    "sub/b.md": "# Psalms\nPs 23 and 1 Cor 13:4-7, 13",
    "-dash.md": "no references",
  };
  for (const [path, text] of Object.entries(notes)) writeFileSync(join(vault, path), text);
  writeFileSync(join(vault, "broken.epub"), "not a zip");

  const cliIndex = new ReferenceIndex(topos);
  const builtin = new ReferenceIndex(topos);
  const reported: string[] = [];
  const paths = [...Object.keys(notes), "broken.epub"];
  const run = runCli(cli, vault, { cache: false, paths }, (path, bytes) => {
    reported.push(path);
    cliIndex.insert(bytes, path, { size: 1, mtime: 1 });
  });
  await run.done;
  for (const [path, text] of Object.entries(notes)) builtin.update(path, text);

  // Every named file is reported (even without references), except one it couldn't read
  assert.deepEqual(reported.sort(), Object.keys(notes).sort());
  assert.ok(cliFailed(run.errors, "broken.epub"), run.errors.join("\n"));
  assert.deepEqual(cliIndex.all().map(simplify), builtin.all().map(simplify));
  assert.equal(cliIndex.all().length, 4);
});

test("a topos that rejects the arguments is an error, not an empty vault", { skip: process.platform === "win32" }, async () => {
  const dir = mkdtempSync(join(tmpdir(), "topos-old-"));
  const old = join(dir, "topos");
  writeFileSync(old, "#!/bin/sh\necho \"error: invalid value 'index' for '--mode <MODE>'\" >&2\nexit 2\n", { mode: 0o755 });
  await assert.rejects(runCli(old, dir, { cache: false, paths: ["a.md"] }, () => {}).done, OutdatedCliError);
  writeFileSync(old, "#!/bin/sh\necho \"error: unexpected argument '--' found\" >&2\nexit 2\n", { mode: 0o755 });
  await assert.rejects(runCli(old, dir, { cache: false, paths: ["a.md"] }, () => {}).done, /unexpected argument/);
});
