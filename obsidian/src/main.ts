import {
  Editor,
  MarkdownView,
  Notice,
  Plugin,
  TFile,
  type Menu,
  type TAbstractFile,
} from "obsidian";
import type { Passage, Topos } from "topos-bible";
import { NO_FILTERS } from "./core/filters.ts";
import { literalWordUrl } from "./core/literalWord.ts";
import { applyReplacements, normalizeReferences, referenceAt } from "./core/references.ts";
import { ReferenceIndex, type Hit } from "./core/search.ts";
import { referenceDecorations } from "./editor/decorations.ts";
import { ReferenceSuggest } from "./editor/suggest.ts";
import { loadTopos } from "./engine.ts";
import { GoToReferenceModal, InsertReferenceModal } from "./modals.ts";
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

  async onload(): Promise<void> {
    await this.loadSettings();
    this.topos = await loadTopos();
    this.index = new ReferenceIndex(this.topos);
    this.search.set({ sort: this.settings.sort, groupBy: this.settings.groupBy });

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
    this.topos?.dispose();
  }

  async loadSettings(): Promise<void> {
    this.settings = { ...DEFAULT_SETTINGS, ...((await this.loadData()) as Partial<ToposSettings>) };
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
    this.indexVersion++;
    for (const listener of this.indexListeners) listener();
  }

  isSearchable(file: TAbstractFile): file is TFile {
    if (!(file instanceof TFile)) return false;
    const extensions = this.settings.extensions.split(",").map((e) => e.trim().replace(/^\./, ""));
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
    this.index.update(file.path, await this.app.vault.cachedRead(file));
    this.notifyIndex();
  }

  /** Searches every file again, in batches so Obsidian stays responsive */
  async reindex(): Promise<void> {
    this.indexing = true;
    this.index = new ReferenceIndex(this.topos);
    this.notifyIndex();
    const files = this.app.vault.getFiles().filter((file) => this.isSearchable(file));
    for (let i = 0; i < files.length; i++) {
      this.index.update(files[i].path, await this.app.vault.cachedRead(files[i]));
      if (i % 50 === 49) await new Promise((resolve) => setTimeout(resolve, 0));
    }
    this.indexing = false;
    this.notifyIndex();
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
