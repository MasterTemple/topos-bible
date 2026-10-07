import { Notice, Platform, type TFile } from "obsidian";
import { IndexStatus } from "topos-bible";
import { cliBatches, cliFailed, OutdatedCliError } from "../core/cli.ts";
import type ToposPlugin from "../main.ts";
import { runCli, type CliRun } from "./cli.ts";

/** The index's folder, in the plugin's folder (which syncs with the vault's settings) */
const INDEX_DIR = "index";
/** The JSON cache of EPUB references from before the index */
const OLD_CACHE = "epub-index.json";
/** Unused EPUB details are deleted only once they're this old, since another device's packs
 * that name them may not have synced yet */
const DETAILS_KEPT_MS = 7 * 24 * 60 * 60 * 1000;
/** Text files searched per batch in the background */
const BATCH_FILES = 200;
const BATCH_CHARS = 4_000_000;

type Kind = "note" | "epub";

function buffer(bytes: Uint8Array): ArrayBuffer {
  return bytes.buffer.slice(bytes.byteOffset, bytes.byteOffset + bytes.byteLength) as ArrayBuffer;
}

/**
 * Keeps the vault's index on disk and up to date
 *
 * - At startup, the packs (this device's and those synced from others) are read, and only the
 *   files that changed since are searched again: by the topos CLI (desktop, with the CLI engine),
 *   else in a background thread; EPUBs the CLI can't read go through EPUB++
 * - Each device writes its own packs, a few seconds after a change; packs from other devices
 *   are read again when Obsidian comes back to the front
 */
export class VaultIndexer {
  private readonly plugin: ToposPlugin;
  /** Bumped by each pass over the vault, so a superseded one stops */
  private pass = 0;
  private cliRun: CliRun | null = null;
  /** The CLI failed this session, so the built-in engine searches instead */
  private cliBroken = false;
  private saveTimer: ReturnType<typeof setTimeout> | undefined;
  private saving: Promise<void> = Promise.resolve();
  /** Other devices' packs as last read, by name: their modification times */
  private readonly packTimes = new Map<string, number>();
  private readonly detailLoads = new Map<string, Promise<void>>();
  /** Files indexed from an edit during a pass, which the pass must not overwrite */
  private readonly edited = new Set<string>();

  constructor(plugin: ToposPlugin) {
    this.plugin = plugin;
  }

  private get dir(): string | null {
    const dir = this.plugin.manifest?.dir;
    return dir ? `${dir}/${INDEX_DIR}` : null;
  }

  private get adapter() {
    return this.plugin.app.vault.adapter;
  }

  private get index() {
    return this.plugin.index;
  }

  /** Whether a file is indexed, and as what */
  kind(file: TFile): Kind | null {
    if (this.plugin.settings.searchEpubs && file.extension.toLowerCase() === "epub" && !this.plugin.isExcluded(file.path))
      return "epub";
    return this.plugin.isSearchable(file) ? "note" : null;
  }

  /** Reads the index from disk and brings it up to date */
  async start(): Promise<void> {
    this.plugin.indexing = true;
    await this.removeOldCache();
    await this.loadPacks(false);
    await this.reconcile();
  }

  stop(): void {
    this.pass++;
    this.cliRun?.stop();
    if (this.saveTimer !== undefined) {
      clearTimeout(this.saveTimer);
      this.saveTimer = undefined;
    }
    void this.save();
  }

  /**
   * Reads the packs (with `changedOnly`, only other devices' packs that changed since they were
   * read); returns whether any were read
   */
  async loadPacks(changedOnly: boolean): Promise<boolean> {
    const dir = this.dir;
    if (!dir || !(await this.adapter.exists(`${dir}/packs`))) return false;
    const own = `${this.plugin.device}-`;
    let loaded = false;
    for (const path of (await this.adapter.list(`${dir}/packs`)).files) {
      const name = path.split("/").pop() ?? path;
      if (!name.endsWith(".bin")) continue;
      const mine = name.startsWith(own);
      if (changedOnly && mine) continue;
      const mtime = (await this.adapter.stat(path))?.mtime ?? 0;
      if (changedOnly && this.packTimes.get(name) === mtime) continue;
      try {
        this.index.core.loadPack(new Uint8Array(await this.adapter.readBinary(path)));
        loaded = true;
      } catch (error) {
        // From another version of the index: its device searches again and rewrites its packs
        const message = (error as { value?: { message?: string } })?.value?.message ?? String(error);
        if (/another version/.test(message)) {
          await this.adapter.remove(path);
          continue;
        }
        console.warn(`topos: ignoring the index pack ${name}`, error);
      }
      if (!mine) this.packTimes.set(name, mtime);
    }
    return loaded;
  }

  /** Reads packs that other devices synced, and searches what they don't cover */
  async refreshFromSync(): Promise<void> {
    if (this.plugin.indexing) return;
    if (await this.loadPacks(true)) await this.reconcile();
  }

  /**
   * Checks every file against the index and searches those that changed (all of them with
   * `force`): at startup, after settings change which files are searched, and after packs sync
   */
  async reconcile(force = false): Promise<void> {
    const pass = ++this.pass;
    const current = () => pass === this.pass;
    this.cliRun?.stop();
    this.edited.clear();
    this.plugin.indexing = true;
    this.plugin.notifyIndexSoon();
    const start = performance.now();
    const { vault } = this.plugin.app;
    const core = this.index.core;
    const notes: TFile[] = [];
    const books: TFile[] = [];
    const wanted: string[] = [];
    let checked = 0;
    for (const file of vault.getFiles()) {
      const kind = this.kind(file);
      if (!kind) continue;
      wanted.push(file.path);
      const { size, mtime } = file.stat;
      const status = force ? IndexStatus.Missing : core.check(file.path, size, mtime);
      if (status === IndexStatus.Fresh) continue;
      if (status === IndexStatus.Unconfirmed) {
        // Synced from another device: a note by its text, a book (too big to read again) by size
        if (kind === "epub" && core.confirm(file.path, size, mtime, null)) continue;
        if (kind === "note") {
          const text = await vault.cachedRead(file);
          if (!current()) return;
          if (core.confirm(file.path, size, mtime, text)) continue;
        }
      }
      (kind === "epub" ? books : notes).push(file);
      if (++checked % 500 === 0) await new Promise((resolve) => setTimeout(resolve, 0));
    }
    core.retain(wanted);
    this.plugin.notifyIndexSoon();
    await this.searchNotes(notes, current);
    await this.searchBooks(books, current);
    if (!current()) return;
    this.plugin.indexing = false;
    this.plugin.notifyIndex();
    this.saveSoon();
    void this.pruneDetails();
    const searched = notes.length + books.length;
    console.debug(
      `topos: ${this.index.referenceCount} references in ${this.index.fileCount} files; searched ${searched} in ${Math.round(performance.now() - start)} ms`,
    );
  }

  /** Indexes a file that was created or edited */
  async update(file: TFile): Promise<void> {
    const kind = this.kind(file);
    if (!kind) return;
    if (kind === "epub") {
      const { size, mtime } = file.stat;
      if (this.index.core.check(file.path, size, mtime) !== IndexStatus.Fresh) await this.searchBooks([file], () => true);
      return;
    }
    if (this.plugin.indexing) this.edited.add(file.path);
    const text = await this.plugin.app.vault.cachedRead(file);
    this.index.update(file.path, text, file.stat);
    this.plugin.notifyIndex();
    this.saveSoon();
  }

  remove(path: string): void {
    this.index.remove(path);
    this.plugin.notifyIndex();
    this.saveSoon();
  }

  rename(file: TFile, oldPath: string): void {
    this.index.rename(oldPath, file.path);
    this.plugin.notifyIndex();
    this.saveSoon();
    // Renamed into or out of what's searched
    if (!this.kind(file)) this.remove(file.path);
    else void this.update(file);
  }

  private async searchNotes(files: TFile[], current: () => boolean): Promise<void> {
    if (files.length === 0) return;
    const done = await this.searchWithCli(files, current);
    if (!current()) return;
    const rest = files.filter((f) => !done.has(f.path));
    const { vault } = this.plugin.app;
    for (let i = 0; i < rest.length; ) {
      const batch = [];
      let chars = 0;
      while (i < rest.length && batch.length < BATCH_FILES && chars < BATCH_CHARS) {
        const file = rest[i++];
        const text = await vault.cachedRead(file);
        batch.push({ path: file.path, size: file.stat.size, mtime: file.stat.mtime, text });
        chars += text.length;
      }
      const entries = await this.plugin.indexFiles(batch);
      if (!current()) return;
      for (const { path, bytes } of entries) {
        const file = vault.getFileByPath(path);
        if (file && !this.edited.has(path)) this.index.insert(bytes, path, file.stat);
      }
      this.plugin.notifyIndexSoon();
      this.saveSoon();
    }
  }

  /**
   * EPUBs: with the CLI, else through EPUB++ (on mobile only when that's allowed, since a phone
   * can't search a library: it uses what another device indexed)
   */
  private async searchBooks(files: TFile[], current: () => boolean): Promise<void> {
    if (files.length === 0) return;
    const done = await this.searchWithCli(files, current);
    if (!current()) return;
    if (!Platform.isDesktopApp && !this.plugin.settings.searchEpubsOnMobile) return;
    for (const file of files) {
      if (done.has(file.path)) continue;
      const references = await this.plugin.epubs.extract(file);
      if (!current()) return;
      if (!references) continue;
      this.index.setEpub(file.path, file.stat, references);
      this.plugin.epubs.refresh([file.path]);
      this.plugin.notifyIndexSoon();
      this.saveSoon();
    }
  }

  /**
   * Searches files with the topos CLI when it's the engine, naming each one (so ignore files
   * can't skip them) in batches that fit on a command line. Returns the paths it indexed.
   */
  private async searchWithCli(files: TFile[], current: () => boolean): Promise<Set<string>> {
    const done = new Set<string>();
    const vaultPath = this.plugin.cliVault();
    if (!vaultPath || this.cliBroken || files.length === 0) return done;
    const { vault } = this.plugin.app;
    for (const batch of cliBatches(files.map((f) => f.path))) {
      let run: CliRun | null = null;
      try {
        run = runCli(this.plugin.cliPath(), vaultPath, { cache: this.plugin.settings.cliCache, paths: batch }, (path, bytes) => {
          const file = vault.getFileByPath(path);
          if (!file || !current() || this.edited.has(path)) return;
          this.index.insert(bytes, path, file.stat);
          done.add(path);
          if (file.extension.toLowerCase() === "epub") this.plugin.epubs.refresh([path]);
          this.plugin.notifyIndexSoon();
        });
        this.cliRun = run;
        await run.done;
      } catch (error) {
        if (!current()) return done;
        this.cliBroken = true;
        const reason = error instanceof OutdatedCliError ? error.message : `topos failed: ${String(error)}`;
        new Notice(`Verse search: ${reason}. Using the built-in engine.`);
        console.warn("topos: the CLI failed", error);
        return done;
      } finally {
        if (run && this.cliRun === run) this.cliRun = null;
      }
      if (!current()) return done;
      const errors = run?.errors ?? [];
      for (const path of batch) if (cliFailed(errors, path)) done.delete(path);
      this.saveSoon();
    }
    return done;
  }

  /** Lets the CLI be tried again (after its settings change) */
  resetCli(): void {
    this.cliBroken = false;
  }

  // Writing

  saveSoon(): void {
    if (this.saveTimer !== undefined) return;
    this.saveTimer = setTimeout(() => {
      this.saveTimer = undefined;
      void this.save();
    }, 3000);
  }

  /** Writes this device's changed packs and new EPUB details */
  save(): Promise<void> {
    this.saving = this.saving.then(async () => {
      const dir = this.dir;
      if (!dir) return;
      const packs = this.index.core.takeDirtyPacks();
      const details = this.index.core.takeUnsavedDetails();
      try {
        if (packs.length > 0 && !(await this.adapter.exists(`${dir}/packs`))) await this.adapter.mkdir(`${dir}/packs`);
        if (details.length > 0 && !(await this.adapter.exists(`${dir}/details`))) await this.adapter.mkdir(`${dir}/details`);
        for (const file of packs) await this.adapter.writeBinary(`${dir}/packs/${file.name}`, buffer(file.bytes));
        for (const file of details) await this.adapter.writeBinary(`${dir}/details/${file.name}`, buffer(file.bytes));
      } catch (error) {
        console.warn("topos: could not save the index", error);
      }
    });
    return this.saving;
  }

  // EPUB details

  /** Reads EPUB details from disk */
  loadDetails(names: string[]): Promise<void> {
    return Promise.all(names.map((name) => this.loadDetail(name))).then(() => undefined);
  }

  /** Reads a book's details, if its references need them */
  async loadFileDetails(path: string): Promise<void> {
    const name = this.index.core.missingFileDetail(path);
    if (name) await this.loadDetail(name);
  }

  private loadDetail(name: string): Promise<void> {
    let load = this.detailLoads.get(name);
    if (!load) {
      load = (async () => {
        const dir = this.dir;
        if (!dir) return;
        try {
          const bytes = await this.adapter.readBinary(`${dir}/details/${name}.bin`);
          this.index.core.loadDetail(name, new Uint8Array(bytes));
        } catch (error) {
          console.warn(`topos: could not read the EPUB details ${name}`, error);
        }
      })().finally(() => this.detailLoads.delete(name));
      this.detailLoads.set(name, load);
    }
    return load;
  }

  /** Deletes details no entry uses (once they're old enough that no pack could still name them) */
  private async pruneDetails(): Promise<void> {
    const dir = this.dir;
    if (!dir || !(await this.adapter.exists(`${dir}/details`))) return;
    const used = new Set(this.index.core.detailNames());
    for (const path of (await this.adapter.list(`${dir}/details`)).files) {
      const name = (path.split("/").pop() ?? "").replace(/\.bin$/, "");
      if (used.has(name)) continue;
      const stat = await this.adapter.stat(path);
      if (stat && Date.now() - stat.mtime > DETAILS_KEPT_MS) await this.adapter.remove(path);
    }
  }

  private async removeOldCache(): Promise<void> {
    const dir = this.plugin.manifest?.dir;
    if (!dir) return;
    try {
      if (await this.adapter.exists(`${dir}/${OLD_CACHE}`)) await this.adapter.remove(`${dir}/${OLD_CACHE}`);
    } catch (error) {
      console.warn("topos: could not remove the old EPUB cache", error);
    }
  }
}
