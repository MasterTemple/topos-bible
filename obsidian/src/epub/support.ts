import { Platform, TFile, type Menu } from "obsidian";
import type { EpubReference } from "topos-bible";
import { bookHits, epubReferences, sectionKey } from "../core/epub.ts";
import { written } from "../core/references.ts";
import type { Hit } from "../core/search.ts";
import { bookStyle } from "../core/settings.ts";
import type ToposPlugin from "../main.ts";
import {
  API_READY_EVENT,
  API_UNLOAD_EVENT,
  EPUB_PLUGIN_ID,
  type EpubAnnotation,
  type EpubPlusPlusApi,
} from "./api.ts";

export const PROVIDER_ID = "topos-bible";

/**
 * References in EPUBs, through EPUB++ (which opens EPUBs in Obsidian): the index finds them (with
 * the topos CLI, or else from the text EPUB++ extracts), and they go back to EPUB++ as
 * annotations it draws, lists, and can save as highlights.
 */
export class EpubSupport {
  api: EpubPlusPlusApi | null = null;
  private unregister: (() => void) | null = null;

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
    this.unregister?.();
    this.unregister = null;
  }

  private connect(api: EpubPlusPlusApi): void {
    if (this.api === api) return;
    this.unregister?.();
    this.api = api;
    this.unregister = this.register(api);
    // Books the CLI couldn't search can go through EPUB++ now
    if (this.enabled && !this.plugin.indexing) void this.plugin.reindex();
  }

  private disconnect(): void {
    this.api = null;
    this.unregister = null;
  }

  /**
   * A book's references from the text EPUB++ extracts, or null without EPUB++. A book it can't
   * read has none (so it isn't tried again until it changes)
   */
  async extract(file: TFile): Promise<EpubReference[] | null> {
    const api = this.api;
    if (!api) return null;
    try {
      // A blank line between paragraphs, so a reference never runs from one into the next
      const book = await api.extractText(file, { blockSeparator: "\n\n" });
      const results = await this.plugin.searchFiles(
        book.sections.map((s) => ({ path: sectionKey(file.path, s.spineIndex), text: s.text })),
      );
      const hits = bookHits(file.path, book.sections, results, (spine, start, end) => book.cfi(spine, start, end));
      return epubReferences(hits);
    } catch (error) {
      console.warn(`topos: could not search ${file.path}`, error);
      return [];
    }
  }

  /** After the setting changes: index EPUBs, or take their references out of the index */
  async toggled(): Promise<void> {
    await this.plugin.reindex();
    this.refresh();
  }

  /** Redraws the annotations in open books (all, or these) */
  refresh(paths?: string[]): void {
    this.api?.refreshAnnotations(PROVIDER_ID, paths);
  }

  /** Opens a reference in its book (its CFI is in the book's details, read if needed) */
  async open(hit: Hit): Promise<boolean> {
    const file = this.plugin.app.vault.getFileByPath(hit.path);
    if (!file || !hit.epub) return false;
    let cfi = hit.epub.cfi;
    if (!cfi) {
      await this.plugin.indexer.loadFileDetails(hit.path);
      cfi = this.plugin.index.get(hit.path).find((h) => h.start === hit.start)?.epub?.cfi ?? "";
    }
    if (this.api) await this.api.open(file, cfi || undefined);
    else await this.plugin.app.workspace.openLinkText(cfi ? `${hit.path}#${cfi}` : hit.path, "", false);
    return true;
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
      annotations: async (file) => {
        if (!this.enabled) return [];
        await plugin.indexer.loadFileDetails(file.path);
        const style = bookStyle(plugin.settings.style);
        return plugin.index
          .get(file.path)
          .filter((hit) => hit.epub?.cfi)
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
