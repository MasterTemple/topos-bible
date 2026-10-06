import { BookStyle, ToposFormat, type Passage, type Topos } from "topos-bible";

/** How references are written: the CLI's --psg-fmt fields (the book style is a separate setting) */
export interface FormatSettings {
  /** Between the book and its chapters */
  bookSeparator: string;
  /** Between a chapter and a verse */
  chapterVerse: string;
  /** Between the ends of a range */
  range: string;
  /** Before another verse in the same chapter */
  verseSeparator: string;
  /** Before a part in another chapter */
  chapterSeparator: string;
  /** `3:16-18`, not `3:16,17,18` */
  joinAdjacent: boolean;
  /** `1-2:3`, not `1:1-2:3` */
  omitFirstVerseOfChapterRange: boolean;
  /** `Jude 1:5`, not `Jude 5` */
  chapterInSingleChapterBooks: boolean;
}

/** The CLI's defaults: `John 3:16,17,18; 4` */
export const DEFAULT_FORMAT: FormatSettings = {
  bookSeparator: " ",
  chapterVerse: ":",
  range: "-",
  verseSeparator: ",",
  chapterSeparator: "; ",
  joinAdjacent: false,
  omitFirstVerseOfChapterRange: false,
  chapterInSingleChapterBooks: true,
};

/** Runs `use` with the engine's format for these settings, then frees it */
export function withFormat<T>(format: FormatSettings, style: BookStyle, use: (format: ToposFormat) => T): T {
  const engine = ToposFormat.create()
    .book(style)
    .bookSeparator(format.bookSeparator)
    .chapterVerse(format.chapterVerse)
    .range(format.range)
    .verseSeparator(format.verseSeparator)
    .chapterSeparator(format.chapterSeparator)
    .joinAdjacent(format.joinAdjacent)
    .omitFirstVerseOfChapterRange(format.omitFirstVerseOfChapterRange)
    .chapterInSingleChapterBooks(format.chapterInSingleChapterBooks);
  try {
    return use(engine);
  } finally {
    engine.dispose();
  }
}

/**
 * A passage written in a book style and format: `John 3:16-18` (OSIS is written as the OSIS
 * reference, `John.3.16-John.3.18`, like the CLI)
 */
export function written(
  topos: Topos,
  passage: Passage,
  style: BookStyle,
  format: FormatSettings = DEFAULT_FORMAT,
): string {
  if (style === BookStyle.Osis) return passage.osis;
  return withFormat(format, style, (f) => topos.formatPassage(passage, f)) ?? passage.reference;
}
