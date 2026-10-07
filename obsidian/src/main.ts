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
import { OffsetUnit, type Passage, type Topos } from "topos-bible";
import type { EditorView } from "@codemirror/view";
import { DEFAULT_FORMAT } from "./core/format.ts";
import { migrateQuery, parseQuery, type SavedQuery } from "./core/query.ts";
import { NO_FILTERS } from "./core/filters.ts";
import { linkUrl, siteName, templateFromTranslation, type LinkBook } from "./core/links.ts";
import { applyReplacements, normalizeReferences, referenceAt } from "./core/references.ts";
import { ReferenceIndex } from "./core/index.ts";
import { searchText, type Hit } from "./core/search.ts";
import { referenceDecorations, refreshReferences } from "./editor/decorations.ts";
import { ReferenceSuggest } from "./editor/suggest.ts";
import { engineWasm, loadTopos } from "./engine.ts";
import { BackgroundSearcher } from "./indexers/background.ts";
import { defaultCliPath } from "./indexers/cli.ts";
import { VaultIndexer } from "./indexers/vault.ts";
import { indexBook, type BookRequest, type BookResult, type IndexRequest } from "./indexers/worker.ts";
import { EpubSupport } from "./epub/support.ts";
import workerSource from "topos-worker-source";
import { GoToReferenceModal, InsertReferenceModal, SavedQueryModal } from "./modals.ts";
import { linkReferences, relinkEditor } from "./reading.ts";
import { bookStyle, DEFAULT_SETTINGS, ToposSettingTab, type ToposSettings } from "./settings.ts";
import { SEARCH_VIEW, SearchView } from "./view/SearchView.tsx";
import { SearchStore } from "./view/store.ts";

export default class ToposPlugin extends Plugin {
  settings: ToposSettings = DEFAULT_SETTINGS;
  topos!: Topos;
  index!: ReferenceIndex;
  search = new SearchStore();
  /** References in EPUBs, through the EPUB++ plugin */
  epubs = new EpubSupport(this);
  /** Bumped whenever the index changes, so the sidebar re-renders */
  indexVersion = 0;
  indexing = false;
  private readonly indexListeners = new Set<() => void>();
  /** The background indexer; null if workers are unavailable (then searches run on this thread) */
  private worker: BackgroundSearcher | null | undefined;
  /** Keeps the index on disk and up to date */
  indexer = new VaultIndexer(this);
  /** Names this device's index packs */
  device = "";

  async onload(): Promise<void> {
    await this.loadSettings();
    this.topos = await loadTopos();
    this.device = this.deviceId();
    this.index = new ReferenceIndex(this.topos, this.device, this.manifest?.version ?? "");
    const { sort, groupBy, context } = this.settings;
    this.search.set({ sort, groupBy, context });
    // Remember the sidebar's order, grouping, and context lines
    this.search.subscribe(() => {
      const { sort, groupBy, context } = this.search.get();
      const settings = this.settings;
      if (sort === settings.sort && groupBy === settings.groupBy && context === settings.context) return;
      Object.assign(settings, { sort, groupBy, context });
      void this.saveData(settings);
    });

    this.registerView(SEARCH_VIEW, (leaf) => new SearchView(leaf, this));
    this.addRibbonIcon("book-open", "Verse search", () => void this.activateSearch());
    this.addSettingTab(new ToposSettingTab(this.app, this));
    this.registerEditorSuggest(new ReferenceSuggest(this));
    this.registerEditorExtension(referenceDecorations(this));
    this.registerMarkdownPostProcessor(linkReferences(this));
    this.registerEvent(this.app.workspace.on("editor-menu", (menu, editor) => this.editorMenu(menu, editor)));
    // The reference color is a CSS variable on each window's body (pop-out windows too)
    this.applyColor();
    this.registerEvent(this.app.workspace.on("window-open", (_win, win) => this.applyColor([win.document])));
    this.addCommands();

    this.app.workspace.onLayoutReady(() => {
      void this.indexer.start();
      this.epubs.start();
      const vault = this.app.vault;
      this.registerEvent(vault.on("modify", (file) => file instanceof TFile && void this.indexer.update(file)));
      this.registerEvent(vault.on("create", (file) => file instanceof TFile && void this.indexer.update(file)));
      this.registerEvent(vault.on("delete", (file) => this.indexer.remove(file.path)));
      this.registerEvent(vault.on("rename", (file, oldPath) => file instanceof TFile && this.indexer.rename(file, oldPath)));
      // Other devices' packs sync in while Obsidian is in the background
      if (typeof document !== "undefined") {
        this.registerDomEvent(document, "visibilitychange", () => {
          if (document.visibilityState === "visible") void this.indexer.refreshFromSync();
        });
        this.registerInterval(window.setInterval(() => void this.indexer.refreshFromSync(), 5 * 60 * 1000));
      }
    });
  }

  onunload(): void {
    for (const doc of this.documents()) doc.body.style.removeProperty("--topos-reference-color");
    this.indexer.stop();
    this.epubs.stop();
    clearTimeout(this.notifyTimer);
    this.worker?.terminate();
    this.topos?.dispose();
  }

  async loadSettings(): Promise<void> {
    const saved = ((await this.loadData()) ?? {}) as Partial<ToposSettings> & {
      joinAdjacent?: boolean;
      translation?: string;
    };
    this.settings = { ...DEFAULT_SETTINGS, ...saved };
    // Format fields added later get their defaults; an earlier joinAdjacent setting carries over
    this.settings.format = {
      ...DEFAULT_FORMAT,
      ...(typeof saved.joinAdjacent === "boolean" ? { joinAdjacent: saved.joinAdjacent } : {}),
      ...saved.format,
    };
    delete (this.settings as { joinAdjacent?: boolean }).joinAdjacent;
    // Before link templates, references opened in Literal Word in a chosen translation
    if (saved.linkTemplate === undefined && typeof saved.translation === "string") {
      this.settings.linkTemplate = templateFromTranslation(saved.translation);
    }
    delete (this.settings as { translation?: string }).translation;
    this.settings.queries = (this.settings.queries ?? []).filter((q) => q && typeof q.name === "string");
    // Saved searches from before 0.4.0 used -o for any overlap
    if ((this.settings.queryFormat ?? 1) < 2) {
      this.settings.queries = this.settings.queries.map((q) => ({ ...q, query: migrateQuery(q.query) }));
      this.settings.queryFormat = 2;
      if (this.settings.queries.length > 0) await this.saveData(this.settings);
    }
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
    this.refreshLinks();
  }

  /**
   * Redraws references everywhere they are shown, after a setting changes how they look or
   * where they link: open editors, reading views, and the sidebar
   */
  refreshLinks(): void {
    this.applyColor();
    this.app.workspace.updateOptions();
    for (const leaf of this.app.workspace.getLeavesOfType("markdown")) {
      if (!(leaf.view instanceof MarkdownView)) continue;
      // Editors keep their decorations until the text or viewport changes, so ask for new ones
      const editor = (leaf.view.editor as { cm?: EditorView } | undefined)?.cm;
      editor?.dispatch({ effects: refreshReferences.of(null) });
      // Callouts, tables, and embedded notes in live preview are rendered once, like reading view
      relinkEditor(this, leaf.view.contentEl);
      // Reading view keeps the HTML it rendered (and linked) until told to render again
      leaf.view.previewMode?.rerender(true);
    }
    this.epubs.refresh();
    this.search.set({});
  }

  // Index

  subscribeIndex = (listener: () => void): (() => void) => {
    this.indexListeners.add(listener);
    return () => this.indexListeners.delete(listener);
  };

  notifyIndex(): void {
    clearTimeout(this.notifyTimer);
    this.notifyTimer = undefined;
    this.lastNotify = performance.now();
    this.indexVersion++;
    for (const listener of this.indexListeners) listener();
  }

  private notifyTimer: ReturnType<typeof setTimeout> | undefined;
  private lastNotify = 0;

  /** Notifies at most twice a second, since the sidebar re-sorts every hit (for indexing in batches) */
  notifyIndexSoon(): void {
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

  /** Whether a path is in one of the folders left out of search */
  isExcluded(path: string): boolean {
    return this.settings.excludeFolders
      .split("\n")
      .map((f) => f.trim().replace(/\/+$/, ""))
      .filter(Boolean)
      .some((folder) => path === folder || path.startsWith(`${folder}/`));
  }

  isSearchable(file: TAbstractFile): file is TFile {
    if (!(file instanceof TFile)) return false;
    return this.extensions().includes(file.extension) && !this.isExcluded(file.path);
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

  /** Searches files into index entries in the background, or on this thread if the worker fails */
  async indexFiles(files: IndexRequest[]): Promise<{ path: string; bytes: Uint8Array }[]> {
    const worker = this.background();
    if (worker) {
      try {
        return await worker.index(files);
      } catch (error) {
        if (this.worker !== worker) return []; // stopped
        console.warn("topos: the background indexer failed, indexing on the main thread", error);
        worker.terminate();
        this.worker = null;
      }
    }
    const entries = [];
    const written = Date.now();
    for (const file of files) {
      const bytes = this.topos.indexEntry(file.path, file.size, file.mtime, file.text, written, OffsetUnit.Utf16);
      entries.push({ path: file.path, bytes });
      await new Promise((resolve) => setTimeout(resolve, 0));
    }
    return entries;
  }

  /** Searches an EPUB into its index entry in the background, or on this thread if the worker fails */
  async indexBook(book: BookRequest): Promise<BookResult> {
    const worker = this.background();
    if (worker) {
      try {
        return await worker.indexBook(book);
      } catch (error) {
        if (this.worker !== worker) return { error: "stopped" };
        console.warn("topos: the background indexer failed, indexing on the main thread", error);
        worker.terminate();
        this.worker = null;
      }
    }
    return indexBook(this.topos, book);
  }

  /** A random name for this device's index packs, kept in this device's storage for the vault */
  private deviceId(): string {
    const key = "topos-bible-device";
    const app = this.app as { loadLocalStorage?: (key: string) => unknown; saveLocalStorage?: (key: string, value: unknown) => void };
    const saved = app.loadLocalStorage?.(key);
    if (typeof saved === "string" && /^[a-z0-9]+$/.test(saved)) return saved;
    const id = Math.random().toString(36).slice(2, 10) || "device";
    app.saveLocalStorage?.(key, id);
    return id;
  }

  /** Searches files in the background, or on this thread if the worker fails */
  async searchFiles(files: { path: string; text: string }[]): Promise<{ path: string; hits: Hit[] }[]> {
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

  /**
   * Brings the index up to date with the vault (after settings change what's searched); with
   * `force`, searches every file again
   */
  async reindex(force = false): Promise<void> {
    this.indexer.resetCli();
    await this.indexer.reconcile(force);
  }

  private reindexTimer: ReturnType<typeof setTimeout> | undefined;

  /** Reindexes once typing in a setting pauses */
  reindexSoon(): void {
    clearTimeout(this.reindexTimer);
    this.reindexTimer = setTimeout(() => void this.reindex(), 1000);
  }

  /** The vault's folder when searching with the topos CLI (the CLI engine, on desktop), else null */
  cliVault(): string | null {
    const adapter = this.app.vault.adapter;
    if (this.settings.engine !== "cli" || !Platform.isDesktopApp || !(adapter instanceof FileSystemAdapter)) return null;
    return adapter.getBasePath();
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
    if (hit.epub && (await this.epubs.open(hit))) return;
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

  /**
   * Whether a click on a reference opens it: a middle click always; on desktop, a Ctrl/Cmd-click
   * (or any click, if the setting allows it); on mobile, which has no Ctrl, a tap outside the
   * editor, or, if that setting is on, a tap in an editor that wasn't focused (the tap that would
   * place the cursor and open the keyboard; once it's open, taps move the cursor). Right clicks
   * never do (they open menus)
   */
  clickOpens(event: MouseEvent, inEditor: boolean, editorFocused = false): boolean {
    if (event.button === 1) return true;
    if (event.button !== 0) return false;
    if (!Platform.isDesktop) return !inEditor || (this.settings.tapOpensInEditor && !editorFocused);
    return !this.settings.clickNeedsModifier || event.ctrlKey || event.metaKey;
  }

  /** The main window's document and each pop-out window's */
  private documents(): Set<Document> {
    const docs = new Set<Document>();
    const main = (this.app.workspace as { containerEl?: HTMLElement }).containerEl?.ownerDocument;
    if (main) docs.add(main);
    this.app.workspace.iterateAllLeaves?.((leaf) => {
      const doc = leaf.view?.containerEl?.ownerDocument;
      if (doc) docs.add(doc);
    });
    return docs;
  }

  /** Sets the references' color in these windows (all of them by default) */
  applyColor(docs: Iterable<Document> = this.documents()): void {
    for (const doc of docs) {
      if (this.settings.referenceColor) doc.body.style.setProperty("--topos-reference-color", this.settings.referenceColor);
      else doc.body.style.removeProperty("--topos-reference-color");
    }
  }

  /** Each book's names, for links */
  private linkBooks: Map<number, LinkBook> | null = null;

  /** The link for a passage, from the link template; null when links are off or it has none */
  referenceUrl(passage: Passage): string | null {
    this.linkBooks ??= new Map(this.topos.books().map((book) => [book.id, book]));
    return linkUrl(this.settings.linkTemplate, passage, this.linkBooks.get(passage.bookId));
  }

  /** Where references open, for menus and tooltips: `BibleHub` */
  linkSite(): string {
    return siteName(this.settings.linkTemplate);
  }

  openReference(passage: Passage): void {
    const url = this.referenceUrl(passage);
    if (url) window.open(url, "_blank");
    else if (!this.settings.linkTemplate.trim()) new Notice("Links are turned off in the Topos settings.");
    else new Notice(`${this.linkSite()} has no link for ${passage.reference}.`);
  }

  /** The reference under the cursor */
  referenceAtCursor(editor: Editor): Passage | null {
    const cursor = editor.getCursor();
    return referenceAt(this.topos, editor.getLine(cursor.line), cursor.ch)?.passage ?? null;
  }

  /** Opens the sidebar showing references that share a verse with this passage */
  async findInVault(passage: Passage): Promise<void> {
    this.search.set({ filters: { ...NO_FILTERS, anyOverlap: [passage.osis] }, scope: "vault" });
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
      name: "Open the reference under the cursor in the browser",
      editorCheckCallback: (checking, editor) => {
        const passage = this.referenceAtCursor(editor);
        if (!passage) return false;
        if (!checking) this.openReference(passage);
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
        .setTitle(`Open ${passage.reference} in ${this.linkSite()}`)
        .setIcon("external-link")
        .onClick(() => this.openReference(passage)),
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

  async copy(text: string): Promise<void> {
    await navigator.clipboard.writeText(text);
    new Notice(`Copied ${text}`);
  }

  /** Rewrites references in the chosen style, in the selection or else the whole note */
  private normalize(editor: Editor): void {
    const style = bookStyle(this.settings.style);
    const selection = editor.getSelection();
    if (selection) {
      const replacements = normalizeReferences(this.topos, selection, style, this.settings.format);
      editor.replaceSelection(applyReplacements(selection, replacements));
      new Notice(`Normalized ${replacements.length} reference${replacements.length === 1 ? "" : "s"}`);
      return;
    }
    const replacements = normalizeReferences(this.topos, editor.getValue(), style, this.settings.format);
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
