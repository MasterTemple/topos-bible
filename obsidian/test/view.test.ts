// Renders the sidebar to HTML with a fake plugin (esbuild compiles the JSX)
import assert from "node:assert/strict";
import { mkdtempSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { test } from "node:test";
import { pathToFileURL } from "node:url";
import esbuild from "esbuild";

test("the sidebar renders filtered, grouped results", async () => {
  const src = (path: string) => JSON.stringify(new URL(`../src/${path}`, import.meta.url).pathname);
  const dir = mkdtempSync(join(tmpdir(), "topos-view-"));
  const entry = join(dir, "entry.tsx");
  writeFileSync(
    entry,
    `import { renderToString } from "react-dom/server";
     import { Topos } from "topos-bible";
     import { SearchApp } from ${src("view/SearchApp.tsx")};
     import { SearchStore } from ${src("view/store.ts")};
     import { ReferenceIndex } from ${src("core/search.ts")};
     import { DEFAULT_SETTINGS } from ${src("core/settings.ts")};
     import { NO_FILTERS } from ${src("core/filters.ts")};

     export function render(filters, groupBy) {
       const topos = Topos.new();
       const index = new ReferenceIndex(topos);
       index.update("a.md", "Read Jn 3:16 and Rom 8:28");
       index.update("b.md", "Psalm 23, then John 1:1");
       const plugin = {
         topos, index, indexVersion: 1, indexing: false,
         settings: DEFAULT_SETTINGS,
         search: new SearchStore({ filters: { ...NO_FILTERS, ...filters }, groupBy }),
         subscribeIndex: () => () => {},
         app: { workspace: { getActiveFile: () => null, on: () => ({}), offref() {} } },
       };
       return renderToString(<SearchApp plugin={plugin} />);
     }`,
  );
  await esbuild.build({
    entryPoints: [entry],
    bundle: true,
    format: "esm",
    platform: "node",
    jsx: "automatic",
    external: ["obsidian", "topos-bible", "react", "react-dom"],
    nodePaths: [new URL("../node_modules", import.meta.url).pathname],
    outfile: join(new URL("../node_modules/.cache/", import.meta.url).pathname, "view.mjs"),
    logLevel: "silent",
  });
  const { render } = await import(
    pathToFileURL(join(new URL("../node_modules/.cache/", import.meta.url).pathname, "view.mjs")).href
  );
  const text = (html: string) => html.replace(/<[^>]+>/g, " ").replace(/\s+/g, " ");

  const all = text(render({}, "file"));
  assert.match(all, /4 references in 2 notes/);
  assert.match(all, /John 3:16/);
  assert.match(all, /Psalms 23/);

  const nt = text(render({ testaments: ["new"] }, "book"));
  assert.match(nt, /3 references in 2 notes/);
  assert.match(nt, /John 2 John 3:16/);
  assert.doesNotMatch(nt, /Psalms/);

  const bad = text(render({ books: ["Jhon"] }, "file"));
  assert.match(bad, /Unknown book &quot;Jhon&quot;/);
});
