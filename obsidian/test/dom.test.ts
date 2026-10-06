// Renders the sidebar in a DOM (happy-dom), so effects run like they do in Obsidian
import assert from "node:assert/strict";
import { mkdirSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { test } from "node:test";
import { pathToFileURL } from "node:url";
import esbuild from "esbuild";
import { Window } from "happy-dom";

const cache = new URL("../node_modules/.cache/", import.meta.url).pathname;

async function loadApp() {
  const src = (path: string) => JSON.stringify(new URL(`../src/${path}`, import.meta.url).pathname);
  mkdirSync(cache, { recursive: true });
  const entry = join(cache, "dom-entry.tsx");
  writeFileSync(
    entry,
    `import { act } from "react";
     import { createRoot } from "react-dom/client";
     import { Topos } from "topos-bible";
     import { SearchApp } from ${src("view/SearchApp.tsx")};
     import { ErrorBoundary } from ${src("view/ErrorBoundary.tsx")};
     import { SearchStore } from ${src("view/store.ts")};
     import { ReferenceIndex } from ${src("core/search.ts")};
     import { DEFAULT_SETTINGS } from ${src("core/settings.ts")};
     export { act };
     export function mount(container, notes, state = {}) {
       const topos = Topos.new();
       const index = new ReferenceIndex(topos);
       for (const [path, text] of Object.entries(notes)) index.update(path, text);
       const plugin = {
         topos, index, indexVersion: 1, indexing: false,
         settings: DEFAULT_SETTINGS,
         search: new SearchStore(state),
         subscribeIndex: () => () => {},
         app: {
           workspace: { getActiveFile: () => null, on: () => ({}), offref() {} },
           vault: { getFileByPath: (path) => ({ path }), cachedRead: async (file) => notes[file.path] },
         },
       };
       const root = createRoot(container);
       act(() => root.render(<ErrorBoundary><SearchApp plugin={plugin} /></ErrorBoundary>));
       return { plugin, root };
     }`,
  );
  const outfile = join(cache, "dom.mjs");
  await esbuild.build({
    entryPoints: [entry],
    bundle: true,
    format: "esm",
    platform: "node",
    jsx: "automatic",
    external: ["obsidian", "topos-bible", "react", "react-dom"],
    nodePaths: [new URL("../node_modules", import.meta.url).pathname],
    outfile,
    logLevel: "silent",
  });
  return import(pathToFileURL(outfile).href);
}

test("the sidebar's inputs and context lines work in a DOM", async () => {
  const window = new Window();
  // Recent Chromium returns a promise from scrollIntoView; React must not see it as a cleanup
  window.HTMLElement.prototype.scrollIntoView = function () {
    return Promise.resolve() as never;
  };
  Object.assign(globalThis, {
    window,
    document: window.document,
    HTMLElement: window.HTMLElement,
    Element: window.Element,
    Node: window.Node,
    Event: window.Event,
    requestAnimationFrame: (f: () => void) => setTimeout(f, 0),
    IS_REACT_ACT_ENVIRONMENT: true,
  });
  Object.defineProperty(globalThis, "navigator", { value: window.navigator, configurable: true });
  const errors: unknown[] = [];
  const consoleError = console.error;
  console.error = (...args: unknown[]) => errors.push(args);
  try {
    const { act, mount } = await loadApp();
    const container = document.createElement("div");
    document.body.appendChild(container);
    const notes = {
      "a.md": "first\nRead Jn 3:16 here\nlast line",
      "b.md": "Rom 8:28",
    };
    const { plugin } = mount(container, notes);
    const text = () => container.textContent!.replace(/\s+/g, " ");
    assert.match(text(), /2 references in 2 notes/);

    // Open the filters and focus the Overlapping input: every book is listed, and arrows move
    const input = [...container.querySelectorAll("input")].find((i) => i.placeholder === "John 3:16-21")!;
    const key = (k: string) =>
      act(() => input.dispatchEvent(new window.KeyboardEvent("keydown", { key: k, bubbles: true }) as never));
    await act(async () => input.focus());
    assert.equal(container.querySelectorAll(".topos-dropdown li").length, plugin.topos.books().length);
    await key("ArrowDown");
    await key("ArrowDown");
    assert.equal(container.querySelector(".topos-dropdown li.is-active span")!.textContent, "Leviticus");

    // Type a reference and press Enter: it becomes a chip and filters the results
    const setter = Object.getOwnPropertyDescriptor(window.HTMLInputElement.prototype, "value")!.set!;
    await act(async () => {
      setter.call(input, "jn 3");
      input.dispatchEvent(new window.Event("input", { bubbles: true }) as never);
    });
    assert.match(container.querySelector(".topos-dropdown li")!.textContent!, /Add "John 3"/);
    await key("Enter");
    assert.deepEqual(plugin.search.get().filters.overlaps, ["John 3"]);
    assert.match(text(), /1 reference in 1 note/);

    // "New search" in the saved-search list clears the filters
    const select = container.querySelector<HTMLSelectElement>('select[aria-label="Saved searches"]')!;
    const clear = [...select.options].find((o) => o.textContent!.startsWith("New search"))!;
    assert.equal(clear.disabled, false);
    await act(async () => {
      select.value = clear.value;
      select.dispatchEvent(new window.Event("change", { bubbles: true }) as never);
    });
    assert.deepEqual(plugin.search.get().filters.overlaps, []);
    assert.match(text(), /2 references in 2 notes/);
    await act(async () => plugin.search.set({ filters: { ...plugin.search.get().filters, overlaps: ["John 3"] } }));

    // Middle-click removes the chip
    const chip = [...container.querySelectorAll(".topos-chip")].find((c) => c.textContent!.startsWith("John 3"))!;
    await act(async () => chip.dispatchEvent(new window.MouseEvent("auxclick", { button: 1, bubbles: true }) as never));
    assert.deepEqual(plugin.search.get().filters.overlaps, []);

    // Context lines are read from the notes and shown around each result
    await act(async () => plugin.search.set({ context: 1 }));
    await act(async () => new Promise((r) => setTimeout(r, 10)));
    const hit = [...container.querySelectorAll(".topos-hit")].find((h) => h.textContent!.includes("John 3:16"))!;
    assert.deepEqual(
      [...hit.querySelectorAll(".topos-context-line")].map((l) => l.textContent),
      ["first", "last line"],
    );

    assert.equal(container.querySelector(".topos-crash"), null);
    assert.deepEqual(errors, []);
  } finally {
    console.error = consoleError;
    await window.happyDOM.close();
  }
});
