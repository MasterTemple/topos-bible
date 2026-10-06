// Loads the built main.js with a stand-in for Obsidian's API (Obsidian itself can't run here)
import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { copyFileSync, mkdirSync } from "node:fs";
import Module, { createRequire } from "node:module";
import { test } from "node:test";

class Events {
  on() {
    return {};
  }
  offref() {}
}
class Plugin {
  app: any;
  manifest: any;
  commands: any[] = [];
  views: string[] = [];
  suggests: any[] = [];
  postProcessors: any[] = [];
  constructor(app: any, manifest: any) {
    this.app = app;
    this.manifest = manifest;
  }
  async loadData() {
    return null;
  }
  async saveData() {}
  registerView(type: string) {
    this.views.push(type);
  }
  addRibbonIcon() {}
  addSettingTab() {}
  registerEditorSuggest(suggest: any) {
    this.suggests.push(suggest);
  }
  registerEditorExtension() {}
  registerMarkdownPostProcessor(p: any) {
    this.postProcessors.push(p);
  }
  registerEvent() {}
  addCommand(command: any) {
    this.commands.push(command);
  }
}
class TFile {
  path: string;
  extension: string;
  constructor(path: string) {
    this.path = path;
    this.extension = path.split(".").pop()!;
  }
}
class EditorSuggest {
  app: any;
  context: any = null;
  constructor(app: any) {
    this.app = app;
  }
  setInstructions() {}
}
const obsidian = {
  Plugin,
  TFile,
  EditorSuggest,
  PluginSettingTab: class {},
  Setting: class {},
  ItemView: class {},
  SuggestModal: class {},
  MarkdownView: class {},
  Notice: class {
    static messages: string[] = [];
    constructor(message: string) {
      obsidian.Notice.messages.push(message);
    }
  },
};

const files: Record<string, string> = {
  "Sermons/a.md": "Read Jn 3:16 and Rom 8:28.",
  "Sermons/b.md": "Psalm 23 and is 2.5%",
  "Archive/old.md": "Gen 1:1",
  "image.png": "Jn 3:16",
};

/** A minimal Editor over one line of text */
function fakeEditor(text: string, ch = text.length) {
  let value = text;
  return {
    getLine: () => value,
    getCursor: () => ({ line: 0, ch }),
    getSelection: () => "",
    getValue: () => value,
    offsetToPos: (offset: number) => ({ line: 0, ch: offset }),
    transaction({ changes }: any) {
      for (const c of [...changes].sort((a: any, b: any) => b.from.ch - a.from.ch)) {
        value = value.slice(0, c.from.ch) + c.text + value.slice(c.to.ch);
      }
    },
    replaceRange(text: string, from: any, to: any) {
      value = value.slice(0, from.ch) + text + value.slice(to.ch);
    },
    setCursor() {},
    get value() {
      return value;
    },
  };
}

test("the built plugin loads, indexes, and runs its commands", async () => {
  execFileSync("node", ["esbuild.config.mjs", "production"], { stdio: "ignore" });
  const require = createRequire(import.meta.url);
  const resolve = (Module as any)._resolveFilename;
  (Module as any)._resolveFilename = function (request: string, ...rest: unknown[]) {
    return request === "obsidian" ? "obsidian" : resolve.call(this, request, ...rest);
  };
  require.cache.obsidian = { id: "obsidian", filename: "obsidian", loaded: true, exports: obsidian } as any;
  // Obsidian loads main.js as CommonJS; this package is "type": "module", so load a .cjs copy
  // (inside node_modules, so it still resolves the CodeMirror packages Obsidian provides)
  mkdirSync(new URL("../node_modules/.cache/", import.meta.url), { recursive: true });
  const bundle = new URL("../node_modules/.cache/main.cjs", import.meta.url);
  copyFileSync(new URL("../main.js", import.meta.url), bundle);
  const ToposPlugin = require(bundle.pathname).default;

  const app = {
    workspace: Object.assign(new Events(), {
      onLayoutReady: (callback: () => void) => callback(),
      updateOptions() {},
      getActiveFile: () => null,
    }),
    vault: Object.assign(new Events(), {
      getFiles: () => Object.keys(files).map((path) => new TFile(path)),
      cachedRead: async (file: TFile) => files[file.path],
    }),
  };
  const plugin = new ToposPlugin(app, {});
  await plugin.onload();
  while (plugin.indexing) await new Promise((r) => setTimeout(r, 5));

  assert.deepEqual(plugin.views, ["topos-bible-search"]);
  assert.deepEqual(
    plugin.index.all().map((h: any) => `${h.path} ${h.passage.reference}`),
    ["Archive/old.md Genesis 1:1", "Sermons/a.md John 3:16", "Sermons/a.md Romans 8:28", "Sermons/b.md Psalms 23"],
  );
  const ids = plugin.commands.map((c: any) => c.id);
  for (const id of ["open-search", "insert-reference", "go-to-reference", "open-literal-word", "normalize-references"]) {
    assert.ok(ids.includes(id), id);
  }

  // Excluding a folder reindexes without it
  plugin.settings.excludeFolders = "Archive";
  await plugin.reindex();
  assert.equal(plugin.index.all().length, 3);

  // Commands that need a reference under the cursor are only available on one
  const copy = plugin.commands.find((c: any) => c.id === "copy-osis");
  assert.equal(copy.editorCheckCallback(true, fakeEditor("See Jn 3:16 here", 6)), true);
  assert.equal(copy.editorCheckCallback(true, fakeEditor("Nothing here", 3)), false);

  // Normalizing rewrites references in the chosen style
  const editor = fakeEditor("See jn 3:16 and 1 cor 13:4");
  plugin.commands.find((c: any) => c.id === "normalize-references").editorCallback(editor);
  assert.equal(editor.value, "See John 3:16 and 1 Corinthians 13:4");

  // Editor autocomplete
  const suggest = plugin.suggests[0];
  const typing = fakeEditor("Read John 3:");
  const trigger = suggest.onTrigger({ line: 0, ch: 12 }, typing);
  assert.deepEqual(trigger.start, { line: 0, ch: 5 });
  suggest.context = { editor: typing, start: trigger.start, end: trigger.end, query: trigger.query };
  const suggestions = suggest.getSuggestions(suggest.context);
  assert.equal(suggestions[0].label, "John 3:1");
  suggest.selectSuggestion(suggestions[15]);
  assert.equal(typing.value, "Read John 3:16");
  assert.equal(suggest.onTrigger({ line: 0, ch: 9 }, fakeEditor("The quick")), null);
});
