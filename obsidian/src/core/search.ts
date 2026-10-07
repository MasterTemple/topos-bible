import { OffsetUnit, type Passage, type Topos } from "topos-bible";

/** A reference found in a file; offsets and columns are UTF-16, like CodeMirror's */
export interface Hit {
  path: string;
  start: number;
  end: number;
  /** 1-based */
  line: number;
  /** 1-based */
  column: number;
  /** The line the reference starts on, for context */
  lineText: string;
  passage: Passage;
  /** For EPUBs (whose offsets run through the book's text, section after section): where it is */
  epub?: EpubLocation;
}

/** Where a reference in an EPUB is: its spine item, its CFI, and the chapter's name */
export interface EpubLocation {
  spineIndex: number;
  cfi: string;
  chapter: string | null;
}

export function searchText(topos: Topos, path: string, text: string): Hit[] {
  return topos.search(text, OffsetUnit.Utf16).map((m) => {
    const lineStart = text.lastIndexOf("\n", m.start - 1) + 1;
    const lineEnd = text.indexOf("\n", m.start);
    return {
      path,
      start: m.start,
      end: m.end,
      line: m.line,
      column: m.column,
      lineText: text.slice(lineStart, lineEnd === -1 ? text.length : lineEnd),
      passage: m.passage,
    };
  });
}
