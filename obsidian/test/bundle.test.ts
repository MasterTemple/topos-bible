// Bundles the engine exactly like the plugin (core module alias + embedded WebAssembly) and runs it
import assert from "node:assert/strict";
import { mkdtempSync, readFileSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { test } from "node:test";
import { pathToFileURL } from "node:url";
import esbuild from "esbuild";

test("the embedded WebAssembly loads in a bundle", async () => {
  const pkg = new URL("../node_modules/topos-bible/", import.meta.url).pathname;
  const dir = mkdtempSync(join(tmpdir(), "topos-bundle-"));
  const entry = join(dir, "entry.ts");
  writeFileSync(
    entry,
    `import { loadTopos } from ${JSON.stringify(new URL("../src/engine.ts", import.meta.url).pathname)};
     import { OffsetUnit } from "topos-bible";
     export async function run() {
       const topos = await loadTopos();
       return topos.search("Read Jn 3:16", OffsetUnit.Utf16).map((m) => m.passage.reference);
     }`,
  );
  const outfile = join(dir, "out.mjs");
  await esbuild.build({
    entryPoints: [entry],
    bundle: true,
    format: "esm",
    platform: "neutral",
    alias: {
      "topos-bible": `${pkg}topos_bible.js`,
      "topos-bible-wasm": `${pkg}topos_bible_bg.wasm`,
    },
    loader: { ".wasm": "binary" },
    outfile,
    logLevel: "silent",
  });
  const { run } = await import(pathToFileURL(outfile).href);
  assert.deepEqual(await run(), ["John 3:16"]);
});

test("the bundled worker loads the engine it is sent and searches", async () => {
  const pkg = new URL("../node_modules/topos-bible/", import.meta.url).pathname;
  const result = await esbuild.build({
    entryPoints: [new URL("../src/indexers/worker.ts", import.meta.url).pathname],
    bundle: true,
    write: false,
    format: "iife",
    alias: { "topos-bible": `${pkg}topos_bible.js` },
    logLevel: "silent",
  });
  // Node's worker threads stand in for a Web Worker: give the script the worker globals it uses
  const shim = `const { parentPort } = require("node:worker_threads");
    globalThis.postMessage = (message) => parentPort.postMessage(message);
    parentPort.on("message", (data) => globalThis.onmessage({ data }));\n`;
  const { Worker } = await import("node:worker_threads");
  const worker = new Worker(shim + result.outputFiles[0].text, { eval: true });
  const replies: unknown[] = [];
  const next = () => new Promise((resolve) => worker.once("message", resolve));
  try {
    worker.postMessage({ type: "init", wasm: readFileSync(`${pkg}topos_bible_bg.wasm`) });
    replies.push(await next());
    worker.postMessage({ type: "search", id: 1, files: [{ path: "a.md", text: "Read Jn 3:16" }] });
    replies.push(await next());
  } finally {
    await worker.terminate();
  }
  assert.deepEqual(replies[0], { type: "ready" });
  const reply = replies[1] as { id: number; results: { path: string; hits: { passage: { reference: string } }[] }[] };
  assert.equal(reply.id, 1);
  assert.deepEqual(reply.results[0].hits.map((h) => h.passage.reference), ["John 3:16"]);
});
