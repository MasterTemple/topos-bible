import { BookStyle } from "topos-bible";
import type { BookCompletion } from "./completions.ts";
import type { Translation } from "./literalWord.ts";
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
  sort: SortOrder;
  groupBy: "file" | "book";
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
};

export function bookStyle(name: StyleName): BookStyle {
  return { name: BookStyle.Name, abbreviation: BookStyle.Abbreviation, osis: BookStyle.Osis }[name];
}
