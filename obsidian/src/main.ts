import {
  Editor,
  FileSystemAdapter,
  MarkdownView,
  Notice,
  Platform,
  Plugin,
  TFile,
  type Menu,
  type TAbstractFile,
} from "obsidian";
import type { Passage, Topos } from "topos-bible";
import { OutdatedCliError } from "./core/cli.ts";
import { parseQuery, type SavedQuery } from "./core/query.ts";
import { NO_FILTERS } from "./core/filters.ts";
import { literalWordUrl } from "./core/literalWord.ts";
import { applyReplacements, normalizeReferences, referenceAt } from "./core/references.ts";
import { ReferenceIndex, searchText, type Hit } from "./core/search.ts";
import { referenceDecorations } from "./editor/decorations.ts";
import { ReferenceSuggest } from "./editor/suggest.ts";
import { engineWasm, loadTopos } from "./engine.ts";
import { BackgroundSearcher } from "./indexers/background.ts";
import { defaultCliPath, runCli, type CliRun } from "./indexers/cli.ts";
import workerSource from "topos-worker-source";
import { GoToReferenceModal, InsertReferenceModal, SavedQueryModal } from "./modals.ts";
import { linkReferences } from "./reading.ts";
import { bookStyle, DEFAULT_SETTINGS, ToposSettingTab, type ToposSettings } from "./settings.ts";
import { SEARCH_VIEW, SearchView } from "./view/SearchView.tsx";
import { SearchStore } from "./view/store.ts";

export default class ToposPlugin extends Plugin {
  settings: ToposSettings = DEFAULT_SETTINGS;
  topos!: Topos;
  index!: ReferenceIndex;
  search = new SearchStore();
  /** Bumped whenever the index changes, so the sidebar re-renders */
  indexVersion = 0;
  indexing = false;
  private readonly indexListeners = new Set<() => void>();
  /** The background indexer; null if workers are unavailable (then searches run on this thread) */
  private worker: BackgroundSearcher | null | undefined;
  /** Bumped by each reindex, so a superseded run stops */
  private generation = 0;
  private cliRun: CliRun | null = null;
  /** Files indexed from an edit during the current reindex, which it must not overwrite */
  private readonly edited = new Set<string>();

  async onload(): Promise<void> {
    await this.loadSettings();
    this.topos = await loadTopos();
    this.index = new ReferenceIndex(this.topos);
    this.search.set({ sort: this.settings.sort, groupBy: this.settings.groupBy });
    // Remember the sidebar's order and grouping
    this.search.subscribe(() => {
      const { sort, groupBy } = this.search.get();
      if (sort === this.settings.sort && groupBy === this.settings.groupBy) return;
      this.settings.sort = sort;
      this.settings.groupBy = groupBy;
      void this.saveData(this.settings);
    });

    this.registerView(SEARCH_VIEW, (leaf) => new SearchView(leaf, this));
    this.addRibbonIcon("book-open", "Verse search", () => void this.activateSearch());
    this.addSettingTab(new ToposSettingTab(this.app, this));
    this.registerEditorSuggest(new ReferenceSuggest(this));
    this.registerEditorExtension(referenceDecorations(this));
    this.registerMarkdownPostProcessor(linkReferences(this));
    this.registerEvent(this.app.workspace.on("editor-menu", (menu, editor) => this.editorMenu(menu, editor)));
    this.addCommands();

    this.app.workspace.onLayoutReady(() => {
      void this.reindex();
      const vault = this.app.vault;
      this.registerEvent(vault.on("modify", (file) => void this.indexFile(file)));
      this.registerEvent(vault.on("create", (file) => void this.indexFile(file)));
      this.registerEvent(
        vault.on("delete", (file) => {
          this.index.remove(file.path);
          this.notifyIndex();
        }),
      );
      this.registerEvent(
        vault.on("rename", (file, oldPath) => {
          this.index.remove(oldPath);
          void this.indexFile(file);
        }),
      );
    });
  }

  onunload(): void {
    this.generation++;
    clearTimeout(this.notifyTimer);
    this.cliRun?.stop();
    this.worker?.terminate();
    this.topos?.dispose();
  }

  async loadSettings(): Promise<void> {
    this.settings = { ...DEFAULT_SETTINGS, ...((await this.loadData()) as Partial<ToposSettings>) };
    this.settings.queries = (this.settings.queries ?? []).filter((q) => q && typeof q.name === "string");
  }

  // Saved searches

  /** Saves a search under a name, replacing one with the same name */
  async saveQuery(query: SavedQuery): Promise<void> {
    const name = query.name.trim();
    if (!name) return;
    const others = this.settings.queries.filter((q) => q.name !== name);
    const index = this.settings.queries.findIndex((q) => q.name === name);
    others.splice(index === -1 ? others.length : index, 0, { name, query: query.query });
    this.settings.queries = others;
    await this.saveData(this.settings);
    this.search.set({}); // re-render the sidebar's list
  }

  async deleteQuery(name: string): Promise<void> {
    this.settings.queries = this.settings.queries.filter((q) => q.name !== name);
    await this.saveData(this.settings);
    this.search.set({});
  }

  /** Shows a saved search in the sidebar; returns the query's mistakes, if any */
  applyQuery(query: SavedQuery): string[] {
    const { filters, folder, errors } = parseQuery(query.query);
    this.search.set(folder ? { filters, scope: "folder", folder } : { filters, scope: "vault", folder: "" });
    return errors;
  }

  async openQuery(query: SavedQuery): Promise<void> {
    const errors = this.applyQuery(query);
    if (errors.length > 0) new Notice(`Saved search "${query.name}": ${errors.join("; ")}`);
    await this.activateSearch();
  }

  async saveSettings(): Promise<void> {
    await this.saveData(this.settings);
    // Re-render editor decorations and reading view with the new settings
    this.app.workspace.updateOptions();
  }

  // Index

  subscribeIndex = (listener: () => void): (() => void) => {
    this.indexListeners.add(listener);
    return () => this.indexListeners.delete(listener);
  };

  private notifyIndex(): void {
    clearTimeout(this.notifyTimer);
    this.notifyTimer = undefined;
    this.lastNotify = performance.now();
    this.indexVersion++;
    for (const listener of this.indexListeners) listener();
  }

  private notifyTimer: ReturnType<typeof setTimeout> | undefined;
  private lastNotify = 0;

  /** Notifies at most twice a second, since the sidebar re-sorts every hit (for indexing in batches) */
  private notifyIndexSoon(): void {
    if (this.notifyTimer !== undefined) return;
    const wait = Math.max(0, 500 - (performance.now() - this.lastNotify));
    this.notifyTimer = setTimeout(() => this.notifyIndex(), wait);
  }

  /** The file extensions to search, from the settings */
  extensions(): string[] {
    return this.settings.extensions
      .split(",")
      .map((e) => e.trim().replace(/^\./, ""))
      .filter(Boolean);
  }

  isSearchable(file: TAbstractFile): file is TFile {
    if (!(file instanceof TFile)) return false;
    const extensions = this.extensions();
    const excluded = this.settings.excludeFolders
      .split("\n")
      .map((f) => f.trim().replace(/\/+$/, ""))
      .filter(Boolean);
    return (
      extensions.includes(file.extension) &&
      !excluded.some((folder) => file.path === folder || file.path.startsWith(`${folder}/`))
    );
  }

  private async indexFile(file: TAbstractFile): Promise<void> {
    if (!this.isSearchable(file)) return;
    if (this.indexing) this.edited.add(file.path);
    const text = await this.app.vault.cachedRead(file);
    const [result] = await this.searchFiles([{ path: file.path, text }]);
    if (!result) return;
    this.index.set(result.path, result.hits);
    this.notifyIndex();
  }

  /** The background indexer, started on first use */
  private background(): BackgroundSearcher | null {
    if (this.worker === undefined) {
      try {
        this.worker = new BackgroundSearcher(workerSource, engineWasm);
      } catch (error) {
        console.warn("topos: no Web Worker, indexing on the main thread", error);
        this.worker = null;
      }
    }
    return this.worker;
  }

  /** Searches files in the background, or on this thread if the worker fails */
  private async searchFiles(files: { path: string; text: string }[]): Promise<{ path: string; hits: Hit[] }[]> {
    const worker = this.background();
    if (worker) {
      try {
        return await worker.search(files);
      } catch (error) {
        if (this.worker !== worker) return []; // stopped
        console.warn("topos: the background indexer failed, indexing on the main thread", error);
        worker.terminate();
        this.worker = null;
      }
    }
    const results = [];
    for (const file of files) {
      results.push({ path: file.path, hits: searchText(this.topos, file.path, file.text) });
      await new Promise((resolve) => setTimeout(resolve, 0));
    }
    return results;
  }

  /** Searches every file again, without blocking Obsidian */
  async reindex(): Promise<void> {
    const generation = ++this.generation;
    this.cliRun?.stop();
    this.cliRun = null;
    this.indexing = true;
    this.edited.clear();
    this.index = new ReferenceIndex(this.topos);
    this.notifyIndex();
    const start = performance.now();
    let engine = "built in";
    if (this.settings.engine === "cli" && Platform.isDesktopApp) {
      try {
        await this.reindexWithCli(generation);
        engine = "CLI";
      } catch (error) {
        if (generation !== this.generation) return;
        const reason = error instanceof OutdatedCliError ? error.message : `topos failed: ${String(error)}`;
        new Notice(`Verse search: ${reason}. Using the built-in engine.`);
        this.index = new ReferenceIndex(this.topos);
        await this.reindexBuiltin(generation);
      }
    } else {
      await this.reindexBuiltin(generation);
    }
    if (generation !== this.generation) return;
    console.debug(`topos: indexed ${this.index.fileCount} files with references (${engine}) in ${Math.round(performance.now() - start)} ms`);
    this.indexing = false;
    this.edited.clear();
    this.notifyIndex();
  }

  /** Reads and searches files in batches, so memory stays bounded and results appear as they come */
  private async reindexBuiltin(generation: number): Promise<void> {
    const files = this.app.vault.getFiles().filter((file) => this.isSearchable(file));
    const maxFiles = 200;
    const maxChars = 4_000_000;
    for (let i = 0; i < files.length; ) {
      const batch: { path: string; text: string }[] = [];
      let chars = 0;
      while (i < files.length && batch.length < maxFiles && chars < maxChars) {
        const file = files[i++];
        const text = await this.app.vault.cachedRead(file);
        batch.push({ path: file.path, text });
        chars += text.length;
      }
      const results = await this.searchFiles(batch);
      if (generation !== this.generation) return;
      for (const { path, hits } of results) if (!this.edited.has(path)) this.index.set(path, hits);
      this.notifyIndexSoon();
    }
  }

  /** Runs the topos CLI over the vault folder, streaming its results into the index */
  private async reindexWithCli(generation: number): Promise<void> {
    const adapter = this.app.vault.adapter;
    if (!(adapter instanceof FileSystemAdapter)) throw new Error("the vault is not a folder on disk");
    const run = runCli(this.cliPath(), adapter.getBasePath(), { cache: this.settings.cliCache, extensions: this.extensions() }, (path, hits) => {
      if (generation !== this.generation || this.edited.has(path)) return;
      const file = this.app.vault.getAbstractFileByPath(path);
      if (!file || !this.isSearchable(file)) return;
      this.index.set(path, hits);
      this.notifyIndexSoon();
    });
    this.cliRun = run;
    try {
      await run.done;
    } finally {
      if (this.cliRun === run) this.cliRun = null;
    }
  }

  /** The topos command to run */
  cliPath(): string {
    return this.settings.cliPath || defaultCliPath();
  }

  /** Checks that the topos command runs, for the settings tab */
  async testCli(): Promise<string> {
    const { execFile } = require("node:child_process") as typeof import("node:child_process");
    const path = this.cliPath();
    return new Promise((resolve) => {
      execFile(path, ["--version"], { timeout: 10_000 }, (error, stdout) => {
        if (error) resolve(`Could not run ${path}: ${error.message}`);
        else resolve(`Found ${stdout.trim() || path}`);
      });
    });
  }

  // Navigation

  async activateSearch(): Promise<void> {
    const { workspace } = this.app;
    let leaf = workspace.getLeavesOfType(SEARCH_VIEW)[0];
    if (!leaf) {
      leaf = workspace.getRightLeaf(false) ?? workspace.getLeaf(true);
      await leaf.setViewState({ type: SEARCH_VIEW, active: true });
    }
    await workspace.revealLeaf(leaf);
  }

  /** Opens the note and selects the reference */
  async openHit(hit: Hit): Promise<void> {
    const file = this.app.vault.getFileByPath(hit.path);
    if (!file) return;
    const leaf = this.app.workspace.getLeaf(false);
    await leaf.openFile(file, { active: true });
    if (leaf.view instanceof MarkdownView) {
      const editor = leaf.view.editor;
      const from = editor.offsetToPos(hit.start);
      const to = editor.offsetToPos(hit.end);
      editor.setSelection(from, to);
      editor.scrollIntoView({ from, to }, true);
      editor.focus();
    }
  }

  openInLiteralWord(passage: Passage): void {
    const url = literalWordUrl(passage, this.settings.translation);
    if (url) window.open(url, "_blank");
    else new Notice("Literal Word only has the 66 books of the Bible.");
  }

  /** The reference under the cursor */
  referenceAtCursor(editor: Editor): Passage | null {
    const cursor = editor.getCursor();
    return referenceAt(this.topos, editor.getLine(cursor.line), cursor.ch)?.passage ?? null;
  }

  /** Opens the sidebar showing references that share a verse with this passage */
  async findInVault(passage: Passage): Promise<void> {
    this.search.set({ filters: { ...NO_FILTERS, overlaps: [passage.osis] }, scope: "vault" });
    await this.activateSearch();
  }

  // Commands and menus

  private addCommands(): void {
    this.addCommand({
      id: "open-search",
      name: "Open verse search",
      callback: () => void this.activateSearch(),
    });
    this.addCommand({
      id: "search-current-note",
      name: "Search references in the current note",
      callback: () => {
        this.search.set({ scope: "file" });
        void this.activateSearch();
      },
    });
    this.addCommand({
      id: "open-saved-search",
      name: "Open a saved search",
      callback: () => {
        if (this.settings.queries.length === 0) new Notice("No saved searches yet: save one from the verse search sidebar.");
        else new SavedQueryModal(this.app, this).open();
      },
    });
    this.addCommand({
      id: "go-to-reference",
      name: "Go to a reference in the vault",
      callback: () => new GoToReferenceModal(this.app, this).open(),
    });
    this.addCommand({
      id: "insert-reference",
      name: "Insert a verse reference",
      editorCallback: (editor) => new InsertReferenceModal(this.app, this, editor).open(),
    });
    this.addCommand({
      id: "find-overlapping",
      name: "Find references to the verses under the cursor",
      editorCheckCallback: (checking, editor) => {
        const passage = this.referenceAtCursor(editor);
        if (!passage) return false;
        if (!checking) void this.findInVault(passage);
        return true;
      },
    });
    this.addCommand({
      id: "open-literal-word",
      name: "Open the reference under the cursor in Literal Word",
      editorCheckCallback: (checking, editor) => {
        const passage = this.referenceAtCursor(editor);
        if (!passage) return false;
        if (!checking) this.openInLiteralWord(passage);
        return true;
      },
    });
    this.addCommand({
      id: "copy-osis",
      name: "Copy the reference under the cursor as OSIS",
      editorCheckCallback: (checking, editor) => {
        const passage = this.referenceAtCursor(editor);
        if (!passage) return false;
        if (!checking) void this.copy(passage.osis);
        return true;
      },
    });
    this.addCommand({
      id: "normalize-references",
      name: "Normalize references in the selection or note",
      editorCallback: (editor) => this.normalize(editor),
    });
  }

  private editorMenu(menu: Menu, editor: Editor): void {
    const passage = this.referenceAtCursor(editor);
    if (!passage) return;
    menu.addSeparator();
    menu.addItem((item) =>
      item
        .setTitle(`Open ${passage.reference} in Literal Word`)
        .setIcon("external-link")
        .onClick(() => this.openInLiteralWord(passage)),
    );
    menu.addItem((item) =>
      item
        .setTitle("Find references to these verses")
        .setIcon("search")
        .onClick(() => void this.findInVault(passage)),
    );
    menu.addItem((item) =>
      item
        .setTitle(`Copy as OSIS (${passage.osis})`)
        .setIcon("copy")
        .onClick(() => void this.copy(passage.osis)),
    );
  }

  private async copy(text: string): Promise<void> {
    await navigator.clipboard.writeText(text);
    new Notice(`Copied ${text}`);
  }

  /** Rewrites references in the chosen style, in the selection or else the whole note */
  private normalize(editor: Editor): void {
    const style = bookStyle(this.settings.style);
    const selection = editor.getSelection();
    if (selection) {
      const replacements = normalizeReferences(this.topos, selection, style);
      editor.replaceSelection(applyReplacements(selection, replacements));
      new Notice(`Normalized ${replacements.length} reference${replacements.length === 1 ? "" : "s"}`);
      return;
    }
    const replacements = normalizeReferences(this.topos, editor.getValue(), style);
    editor.transaction({
      changes: replacements.map((r) => ({
        from: editor.offsetToPos(r.start),
        to: editor.offsetToPos(r.end),
        text: r.text,
      })),
    });
    new Notice(`Normalized ${replacements.length} reference${replacements.length === 1 ? "" : "s"}`);
  }
}
