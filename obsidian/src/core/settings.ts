import { BookStyle } from "topos-bible";
import type { BookCompletion } from "./completions.ts";
import type { Translation } from "./literalWord.ts";
import type { SavedQuery } from "./query.ts";
import type { SortOrder } from "./sort.ts";

export type StyleName = "name" | "abbreviation" | "osis";

export interface ToposSettings {
  /** How references are written by autocomplete, normalizing, and the sidebar */
  style: StyleName;
  /** Literal Word translation ("" uses Literal Word's default) */
  translation: Translation;
  /** Underline references in the editor and make them clickable */
  linkInEditor: boolean;
  /** Turn references into links in reading view */
  linkInReading: boolean;
  /** In the editor, only open Literal Word with Ctrl/Cmd-click */
  clickNeedsModifier: boolean;
  /** Suggest chapters, verses, and books while typing */
  autocomplete: boolean;
  /** Book names in prose would trigger on every word, so by default they need a capital */
  bookCompletion: BookCompletion;
  suggestionLimit: number;
  /** File extensions to search, separated by commas */
  extensions: string;
  /** Folders to leave out of search, one per line */
  excludeFolders: string;
  /** The sidebar's order and grouping, remembered between sessions */
  sort: SortOrder;
  groupBy: "file" | "book";
  /** Lines of context around each result in the sidebar */
  context: number;
  /** Named searches, with the CLI's filter options */
  queries: SavedQuery[];
  /** Index the vault in a background thread, or with the topos CLI (desktop only) */
  engine: "builtin" | "cli";
  /** Path to the topos command ("" finds it in ~/.cargo/bin or PATH) */
  cliPath: string;
  /** Let the CLI reuse results for unchanged files between runs */
  cliCache: boolean;
}

export const DEFAULT_SETTINGS: ToposSettings = {
  style: "name",
  translation: "",
  linkInEditor: true,
  linkInReading: true,
  clickNeedsModifier: true,
  autocomplete: true,
  bookCompletion: "capitalized",
  suggestionLimit: 20,
  extensions: "md, txt",
  excludeFolders: "",
  sort: "file",
  groupBy: "file",
  context: 0,
  queries: [],
  engine: "builtin",
  cliPath: "",
  cliCache: true,
};

export function bookStyle(name: StyleName): BookStyle {
  return { name: BookStyle.Name, abbreviation: BookStyle.Abbreviation, osis: BookStyle.Osis }[name];
}
