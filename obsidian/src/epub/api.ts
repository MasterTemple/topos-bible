// The parts of EPUB++'s plugin API (its `src/api.ts`) that topos uses. EPUB++ opens EPUBs in
// Obsidian; topos searches the text it extracts and hands the references back as annotations.
import type { Menu, TFile } from "obsidian";

export const EPUB_PLUGIN_ID = "epub-plus-plus";
export const API_READY_EVENT = "epub-plus-plus:api-ready";
export const API_UNLOAD_EVENT = "epub-plus-plus:api-unload";

export interface EpubPlusPlusApi {
  readonly version: number;
  extractText(file: TFile, opts?: { blockSeparator?: string }): Promise<ExtractedBook>;
  registerAnnotationProvider<T>(provider: AnnotationProvider<T>): () => void;
  refreshAnnotations(providerId: string, paths?: string[]): void;
  open(file: TFile, locator?: string): Promise<void>;
}

export interface ExtractedBook {
  sections: { spineIndex: number; href: string; text: string; title: string | null }[];
  cfi(spineIndex: number, start: number, end: number): string | null;
}

export interface EpubAnnotation<T = unknown> {
  id: string;
  locator: string;
  label: string;
  color?: string;
  data?: T;
}

export interface AnnotationContext {
  file: TFile;
  text: string;
  chapter: string | null;
}

export interface AnnotationProvider<T = unknown> {
  id: string;
  name: string;
  icon?: string;
  color?: string;
  style?: (color: string) => string;
  annotations(file: TFile): EpubAnnotation<T>[] | Promise<EpubAnnotation<T>[]>;
  onClick?(annotations: EpubAnnotation<T>[], event: MouseEvent, ctx: AnnotationContext): boolean | void;
  menu?(menu: Menu, annotations: EpubAnnotation<T>[], ctx: AnnotationContext): void;
  tooltip?(annotation: EpubAnnotation<T>, ctx: AnnotationContext): string | null;
}
