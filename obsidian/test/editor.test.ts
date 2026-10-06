// Runs the editor's reference decorations in a real CodeMirror view (in happy-dom)
import assert from "node:assert/strict";
import { mkdirSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { test } from "node:test";
import { pathToFileURL } from "node:url";
import esbuild from "esbuild";
import { Window } from "happy-dom";

const cache = new URL("../node_modules/.cache/", import.meta.url).pathname;

async function load() {
  const src = (path: string) => JSON.stringify(new URL(`../src/${path}`, import.meta.url).pathname);
  mkdirSync(cache, { recursive: true });
  const entry = join(cache, "editor-entry.ts");
  writeFileSync(
    entry,
    `export { EditorView } from "@codemirror/view";
     export { referenceDecorations, refreshReferences } from ${src("editor/decorations.ts")};
     export { linkUrl } from ${src("core/links.ts")};
     export { DEFAULT_SETTINGS } from ${src("core/settings.ts")};`,
  );
  const outfile = join(cache, "editor.mjs");
  await esbuild.build({
    entryPoints: [entry],
    bundle: true,
    format: "esm",
    platform: "node",
    external: ["topos-bible"],
    nodePaths: [new URL("../node_modules", import.meta.url).pathname],
    outfile,
    logLevel: "silent",
  });
  return import(pathToFileURL(outfile).href);
}

test("editor links follow the settings after a refresh", async () => {
  const window = new Window();
  Object.assign(globalThis, {
    window,
    document: window.document,
    MutationObserver: window.MutationObserver,
    requestAnimationFrame: (f: () => void) => setTimeout(f, 0),
    cancelAnimationFrame: (id: number) => clearTimeout(id),
  });
  Object.defineProperty(globalThis, "navigator", { value: window.navigator, configurable: true });
  const { EditorView, referenceDecorations, refreshReferences, linkUrl, DEFAULT_SETTINGS } = await load();
  const { Topos } = await import("topos-bible");
  const topos = Topos.new();
  const settings = { ...DEFAULT_SETTINGS };
  const plugin = { topos, settings, referenceUrl: (passage: any) => linkUrl(settings.linkTemplate, passage) };
  const view = new EditorView({
    doc: "Read Jn 3:16 today",
    extensions: referenceDecorations(plugin),
    parent: document.body,
  });
  const links = () =>
    [...view.contentDOM.querySelectorAll(".topos-reference")].map((el: any) => el.getAttribute("data-topos-url"));
  assert.deepEqual(links(), ["https://app.literalword.com/43/3/16"]);

  settings.linkTemplate = "https://lets.bible/bible/{book|lower|kebab}/{chapter}[?v={verse}]";
  // Without a refresh, the editor keeps the link it drew
  assert.deepEqual(links(), ["https://app.literalword.com/43/3/16"]);
  view.dispatch({ effects: refreshReferences.of(null) });
  assert.deepEqual(links(), ["https://lets.bible/bible/john/3?v=16"]);

  // No links: still underlined, with nothing to open
  settings.linkTemplate = "";
  view.dispatch({ effects: refreshReferences.of(null) });
  assert.deepEqual(links(), [null]);
  settings.linkInEditor = false;
  view.dispatch({ effects: refreshReferences.of(null) });
  assert.deepEqual(links(), []);
  view.destroy();
});
