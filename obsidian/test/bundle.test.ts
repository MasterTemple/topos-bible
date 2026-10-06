// Bundles the engine exactly like the plugin (core module alias + embedded WebAssembly) and runs it
import assert from "node:assert/strict";
import { mkdtempSync, writeFileSync } from "node:fs";
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
