import { Platform, TFile, type Menu } from "obsidian";
import { cliFailed } from "../core/cli.ts";
import { bookHits, cliBatches, EpubCache, sectionKey } from "../core/epub.ts";
import { written } from "../core/references.ts";
import type { Hit } from "../core/search.ts";
import { bookStyle } from "../core/settings.ts";
import { runCli, type CliRun } from "../indexers/cli.ts";
import type ToposPlugin from "../main.ts";
import {
  API_READY_EVENT,
  API_UNLOAD_EVENT,
  EPUB_PLUGIN_ID,
  type EpubAnnotation,
  type EpubPlusPlusApi,
} from "./api.ts";

export const PROVIDER_ID = "topos-bible";
const CACHE_FILE = "epub-index.json";

/**
 * References in EPUBs, through EPUB++ (which opens EPUBs in Obsidian): with the CLI engine, the
 * topos CLI searches the books (in parallel, off Obsidian's thread); otherwise EPUB++ extracts each
 * book's text and topos searches it like a note. The references go back to EPUB++ as annotations
 * it draws, lists, and can save as highlights.
 */
export class EpubSupport {
  api: EpubPlusPlusApi | null = null;
  private unregister: (() => void) | null = null;
  private cache: EpubCache | null = null;
  private saveTimer: ReturnType<typeof setTimeout> | undefined;
  /** Bumped by each full pass, so a superseded one stops */
  private pass = 0;
  /** The CLI searching books, stopped by a new pass */
  private cliRun: CliRun | null = null;

  private readonly plugin: ToposPlugin;

  constructor(plugin: ToposPlugin) {
    this.plugin = plugin;
  }

  get enabled(): boolean {
    return this.plugin.settings.searchEpubs;
  }

  /** Connects to EPUB++ now or whenever it loads */
  start(): void {
    const { workspace } = this.plugin.app;
    const on = workspace.on.bind(workspace) as unknown as (name: string, cb: (...args: unknown[]) => void) => ReturnType<typeof workspace.on>;
    this.plugin.registerEvent(on(API_READY_EVENT, (api) => this.connect(api as EpubPlusPlusApi)));
    this.plugin.registerEvent(on(API_UNLOAD_EVENT, () => this.disconnect()));
    const plugins = (this.plugin.app as { plugins?: { plugins?: Record<string, { api?: EpubPlusPlusApi }> } }).plugins?.plugins;
    const api = plugins?.[EPUB_PLUGIN_ID]?.api;
    if (api) this.connect(api);
  }

  stop(): void {
    this.pass++;
    this.cliRun?.stop();
    this.unregister?.();
    this.unregister = null;
    if (this.saveTimer !== undefined) {
      clearTimeout(this.saveTimer);
      void this.saveCache();
    }
  }

  private connect(api: EpubPlusPlusApi): void {
    if (this.api === api) return;
    this.unregister?.();
    this.api = api;
    this.unregister = this.register(api);
    if (this.enabled && !this.plugin.indexing) void this.indexAll();
  }

  private disconnect(): void {
    this.api = null;
    this.unregister = null;
  }

  isEpub(file: TFile): boolean {
    return file.extension.toLowerCase() === "epub" && !this.plugin.isExcluded(file.path);
  }

  /** Indexes every EPUB (from the cache when unchanged); `generation` stops it when a reindex starts */
  async indexAll(generation = this.plugin.generation): Promise<void> {
    const api = this.api;
    if (!api || !this.enabled) return;
    const pass = ++this.pass;
    this.cliRun?.stop();
    const current = () => pass === this.pass && generation === this.plugin.generation && this.api === api;
    const cache = await this.loadCache();
    const files = this.plugin.app.vault.getFiles().filter((file) => this.isEpub(file));
    const changed = files.filter((file) => {
      const hits = cache.get(file.path, file.stat);
      if (hits) this.show(file, hits);
      return !hits;
    });
    const searched = await this.searchWithCli(changed, current);
    for (const file of changed) {
      if (!current()) return;
      if (!searched.has(file.path)) await this.indexFile(file, false);
    }
  }

  /** Searches one EPUB (or takes its references from the cache) and shows them */
  async indexFile(file: TFile, cli = true): Promise<void> {
    const api = this.api;
    if (!api || !this.enabled || !this.isEpub(file)) return;
    const hits = (await this.loadCache()).get(file.path, file.stat);
    if (hits) return this.show(file, hits);
    if (cli && (await this.searchWithCli([file], () => this.api === api)).has(file.path)) return;
    try {
      this.store(file, await this.search(api, file));
    } catch (error) {
      console.warn(`topos: could not search ${file.path}`, error);
    }
  }

  /**
   * Searches books with the topos CLI when it's the engine, naming each one (so ignore files
   * don't skip them) in batches that fit on a command line. Returns the paths it searched; the
   * others (it couldn't read them, or the CLI failed or is too old) are left to EPUB++.
   */
  private async searchWithCli(files: TFile[], current: () => boolean): Promise<Set<string>> {
    const searched = new Set<string>();
    const vault = this.plugin.cliVault();
    if (!vault || files.length === 0) return searched;
    for (const batch of cliBatches(files.map((f) => f.path))) {
      const found = new Map<string, Hit[]>();
      const options = { cache: this.plugin.settings.cliCache, extensions: [], paths: batch };
      let run: CliRun | null = null;
      try {
        run = runCli(this.plugin.cliPath(), vault, options, (path, hits) => found.set(path, hits));
        this.cliRun = run;
        await run.done;
      } catch (error) {
        if (current()) console.warn("topos: the CLI could not search EPUBs, using EPUB++", error);
        return searched;
      } finally {
        if (run && this.cliRun === run) this.cliRun = null;
      }
      if (!current()) return searched;
      // A topos from before EPUB positions: its references can't be shown in books
      if ([...found.values()].some((hits) => hits.some((hit) => !hit.epub))) {
        console.warn("topos: this topos CLI doesn't report where references are in EPUBs; update it with cargo install");
        return searched;
      }
      for (const path of batch) {
        const file = this.plugin.app.vault.getFileByPath(path);
        if (!file || cliFailed(run.errors, path)) continue;
        this.store(file, found.get(path) ?? []);
        searched.add(path);
      }
    }
    return searched;
  }

  /** Caches a book's references and shows them */
  private store(file: TFile, hits: Hit[]): void {
    this.cache?.set(file.path, file.stat, hits);
    this.saveCacheSoon();
    this.show(file, hits);
  }

  private show(file: TFile, hits: Hit[]): void {
    this.plugin.index.set(file.path, hits);
    this.plugin.notifyIndexSoon();
    this.api?.refreshAnnotations(PROVIDER_ID, [file.path]);
  }

  private async search(api: EpubPlusPlusApi, file: TFile): Promise<Hit[]> {
    // A blank line between paragraphs, so a reference never runs from one into the next
    const book = await api.extractText(file, { blockSeparator: "\n\n" });
    const results = await this.plugin.searchFiles(
      book.sections.map((s) => ({ path: sectionKey(file.path, s.spineIndex), text: s.text })),
    );
    return bookHits(file.path, book.sections, results, (spine, start, end) => book.cfi(spine, start, end));
  }

  remove(path: string): void {
    this.cache?.delete(path);
    this.saveCacheSoon();
  }

  rename(oldPath: string, newPath: string): void {
    this.cache?.rename(oldPath, newPath);
    this.saveCacheSoon();
  }

  /** After the setting changes: index EPUBs, or take their references out of the index */
  async toggled(): Promise<void> {
    if (this.enabled) return this.indexAll();
    this.pass++;
    this.cliRun?.stop();
    for (const file of this.plugin.app.vault.getFiles()) if (file.extension.toLowerCase() === "epub") this.plugin.index.remove(file.path);
    this.plugin.notifyIndexSoon();
    this.api?.refreshAnnotations(PROVIDER_ID);
  }

  /** Redraws the annotations in open books (after the reference style or color changes) */
  refresh(): void {
    this.api?.refreshAnnotations(PROVIDER_ID);
  }

  async open(hit: Hit): Promise<boolean> {
    const file = this.plugin.app.vault.getFileByPath(hit.path);
    if (!file || !hit.epub) return false;
    if (this.api) await this.api.open(file, hit.epub.cfi);
    else await this.plugin.app.workspace.openLinkText(`${hit.path}#${hit.epub.cfi}`, "", false);
    return true;
  }

  // Cache, in the plugin's folder

  private cachePath(): string | null {
    const dir = this.plugin.manifest?.dir;
    return dir ? `${dir}/${CACHE_FILE}` : null;
  }

  private async loadCache(): Promise<EpubCache> {
    if (this.cache) return this.cache;
    const engine = this.plugin.manifest?.version ?? "";
    let json: unknown = null;
    const path = this.cachePath();
    try {
      if (path && (await this.plugin.app.vault.adapter.exists(path))) json = JSON.parse(await this.plugin.app.vault.adapter.read(path));
    } catch (error) {
      console.warn("topos: ignoring the EPUB cache", error);
    }
    this.cache ??= EpubCache.from(json, engine);
    return this.cache;
  }

  private saveCacheSoon(): void {
    if (this.saveTimer !== undefined) return;
    this.saveTimer = setTimeout(() => void this.saveCache(), 2000);
  }

  private async saveCache(): Promise<void> {
    this.saveTimer = undefined;
    const path = this.cachePath();
    if (!this.cache?.dirty || !path) return;
    await this.plugin.app.vault.adapter.write(path, JSON.stringify(this.cache));
  }

  // The annotation provider

  private register(api: EpubPlusPlusApi): () => void {
    const plugin = this.plugin;
    const color = () => {
      const body = activeDocument.body;
      const get = (name: string) => getComputedStyle(body).getPropertyValue(name).trim();
      return plugin.settings.referenceColor || get("--interactive-accent") || "#7c3aed";
    };
    const passageOf = (a: EpubAnnotation) => (a.data as Hit).passage;
    return api.registerAnnotationProvider<Hit>({
      id: PROVIDER_ID,
      name: "Bible references",
      icon: "book-open",
      get color() {
        return color();
      },
      // Like references in notes: a soft glow and a dashed underline
      style: (c) =>
        `text-decoration: underline dashed 1.5px ${c}; text-underline-offset: 3px; text-shadow: 0 0 8px color-mix(in srgb, ${c} 55%, transparent);`,
      annotations: (file) => {
        if (!this.enabled) return [];
        const style = bookStyle(plugin.settings.style);
        return plugin.index
          .get(file.path)
          .filter((hit) => hit.epub)
          .map((hit) => ({
            id: String(hit.start),
            locator: hit.epub!.cfi,
            label: written(plugin.topos, hit.passage, style, plugin.settings.format),
            data: hit,
          }));
      },
      // Ctrl/Cmd-click or a middle click opens the reference (taps on mobile open the menu)
      onClick: (annotations, event) => {
        if (!Platform.isDesktop || !plugin.clickOpens(event, false)) return false;
        plugin.openReference(passageOf(annotations[0]!));
        return true;
      },
      menu: (menu: Menu, annotations) => {
        for (const a of annotations) {
          const passage = passageOf(a);
          const section = `topos-${a.id}`;
          if (plugin.referenceUrl(passage))
            menu.addItem((item) =>
              item
                .setTitle(`Open ${a.label} in ${plugin.linkSite()}`)
                .setIcon("external-link")
                .setSection(section)
                .onClick(() => plugin.openReference(passage)),
            );
          menu.addItem((item) =>
            item
              .setTitle("Find references to these verses")
              .setIcon("search")
              .setSection(section)
              .onClick(() => void plugin.findInVault(passage)),
          );
          menu.addItem((item) =>
            item
              .setTitle(`Copy as OSIS (${passage.osis})`)
              .setIcon("copy")
              .setSection(section)
              .onClick(() => void plugin.copy(passage.osis)),
          );
        }
      },
      tooltip: (a) => {
        const site = plugin.referenceUrl(passageOf(a)) ? plugin.linkSite() : "";
        if (!site || !Platform.isDesktop) return a.label;
        return `${a.label}\n${plugin.settings.clickNeedsModifier ? "Ctrl/Cmd-click" : "Click"} to open in ${site}`;
      },
    });
  }
}
