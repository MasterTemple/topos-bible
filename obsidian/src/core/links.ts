import type { BookInfo, Passage } from "topos-bible";

/**
 * Links to Bible websites, from a template like
 * `https://biblehub.com/{book.biblehub}/{chapter}[-{verse}].htm`.
 *
 * - `{name}` is a value of the reference (see {@link PLACEHOLDERS}), URL-encoded, and
 *   `{name|lower}` changes it first (see {@link FILTERS})
 * - `[...]` is left out when a placeholder inside it has no value, like `{verse}` for a whole
 *   chapter: `{chapter}[-{verse}]` is `3-16` for John 3:16 and `3` for John 3
 * - A placeholder with no value outside `[...]` means there is no link
 */

/** A site references can open in */
export interface LinkSite {
  id: string;
  name: string;
  template: string;
}

/** Sites with their templates ready to use */
export const SITES: LinkSite[] = [
  { id: "literalword", name: "Literal Word", template: "https://app.literalword.com/{book.id}/{chapter}[/{verse}]" },
  { id: "biblehub", name: "BibleHub", template: "https://biblehub.com/{book.biblehub}/{chapter}[-{verse}].htm" },
  { id: "biblegateway", name: "BibleGateway", template: "https://www.biblegateway.com/passage/?search={reference}" },
  {
    id: "youversion",
    name: "YouVersion",
    template: "https://www.bible.com/bible/59/{book.usfm}.{chapter}[.{verse}][-{end_verse}]",
  },
];

/** What each placeholder is, for the settings */
export const PLACEHOLDERS: Record<string, string> = {
  book: "the book's name: 1 Corinthians",
  "book.name": "the same as {book}",
  "book.abbreviation": "its abbreviation: 1 Cor",
  "book.osis": "its OSIS id: 1Cor",
  "book.id": "its number: 46",
  "book.usfm": "its USFM code (YouVersion): 1CO",
  "book.biblehub": "its BibleHub name: 1_corinthians",
  chapter: "the first chapter: 13",
  verse: "the first verse (none for a whole chapter): 4",
  end_chapter: "the last chapter of the first part, when it spans chapters",
  end_verse: "the last verse of the first part, when it is a range in one chapter: 7",
  reference: "the whole reference: 1 Corinthians 13:4-7",
  osis: "the whole reference in OSIS: 1Cor.13.4-1Cor.13.7",
};

/** Changes to a value: `{book|lower}` */
export const FILTERS: Record<string, (value: string) => string> = {
  lower: (value) => value.toLowerCase(),
  upper: (value) => value.toUpperCase(),
  /** Spaces become `_`: `1_corinthians` */
  snake: (value) => value.replace(/\s+/g, "_"),
  /** Spaces become `-`: `1-corinthians` */
  kebab: (value) => value.replace(/\s+/g, "-"),
  /** Spaces are removed: `1corinthians` */
  compact: (value) => value.replace(/\s+/g, ""),
};

/** USFM (Paratext) book codes, which YouVersion uses: `JHN`, `1CO` */
const USFM = [
  "GEN", "EXO", "LEV", "NUM", "DEU", "JOS", "JDG", "RUT", "1SA", "2SA", "1KI", "2KI", "1CH", "2CH",
  "EZR", "NEH", "EST", "JOB", "PSA", "PRO", "ECC", "SNG", "ISA", "JER", "LAM", "EZK", "DAN", "HOS",
  "JOL", "AMO", "OBA", "JON", "MIC", "NAM", "HAB", "ZEP", "HAG", "ZEC", "MAL", "MAT", "MRK", "LUK",
  "JHN", "ACT", "ROM", "1CO", "2CO", "GAL", "EPH", "PHP", "COL", "1TH", "2TH", "1TI", "2TI", "TIT",
  "PHM", "HEB", "JAS", "1PE", "2PE", "1JN", "2JN", "3JN", "JUD", "REV",
];

/** BibleHub's book names (`biblehub.com/1_corinthians/13-4.htm`) */
const BIBLEHUB = [
  "genesis", "exodus", "leviticus", "numbers", "deuteronomy", "joshua", "judges", "ruth",
  "1_samuel", "2_samuel", "1_kings", "2_kings", "1_chronicles", "2_chronicles", "ezra", "nehemiah",
  "esther", "job", "psalms", "proverbs", "ecclesiastes", "songs", "isaiah", "jeremiah",
  "lamentations", "ezekiel", "daniel", "hosea", "joel", "amos", "obadiah", "jonah", "micah",
  "nahum", "habakkuk", "zephaniah", "haggai", "zechariah", "malachi", "matthew", "mark", "luke",
  "john", "acts", "romans", "1_corinthians", "2_corinthians", "galatians", "ephesians",
  "philippians", "colossians", "1_thessalonians", "2_thessalonians", "1_timothy", "2_timothy",
  "titus", "philemon", "hebrews", "james", "1_peter", "2_peter", "1_john", "2_john", "3_john",
  "jude", "revelation",
];

/** The book details a template can use (from `Topos.books()`) */
export type LinkBook = Pick<BookInfo, "id" | "name" | "abbreviation" | "osis">;

/** Each placeholder's value for a passage (missing when it has none) */
function values(passage: Passage, book: LinkBook | undefined): Record<string, string | undefined> {
  const first = passage.segments[0];
  const known = passage.bookId >= 1 && passage.bookId <= 66;
  const result: Record<string, string | undefined> = {
    book: book?.name ?? passage.book,
    "book.name": book?.name ?? passage.book,
    "book.abbreviation": book?.abbreviation,
    "book.osis": book?.osis,
    "book.id": String(passage.bookId),
    "book.usfm": known ? USFM[passage.bookId - 1] : undefined,
    "book.biblehub": known ? BIBLEHUB[passage.bookId - 1] : undefined,
    reference: passage.reference,
    osis: passage.osis,
  };
  if (first?.tag === "Verses") {
    result.chapter = String(first.start.chapter);
    result.verse = String(first.start.verse);
    if (first.end && first.end.chapter === first.start.chapter) result.end_verse = String(first.end.verse);
    if (first.end && first.end.chapter !== first.start.chapter) result.end_chapter = String(first.end.chapter);
  } else if (first) {
    result.chapter = String(first.start);
    if (first.end !== null && first.end !== first.start) result.end_chapter = String(first.end);
  }
  return result;
}

type Part = { text: string } | { name: string; filters: string[] } | { optional: Part[] };

/** A template's parts, or what is wrong with it */
function parse(template: string): Part[] | string {
  const top: Part[] = [];
  let parts = top;
  let text = "";
  const flush = () => {
    if (text) parts.push({ text });
    text = "";
  };
  for (let i = 0; i < template.length; i++) {
    const c = template[i];
    if (c === "{") {
      const close = template.indexOf("}", i);
      if (close < 0) return "a { has no }";
      flush();
      const [name = "", ...filters] = template
        .slice(i + 1, close)
        .split("|")
        .map((s) => s.trim());
      if (!(name in PLACEHOLDERS)) return `unknown placeholder {${name}}`;
      const unknown = filters.find((f) => !(f in FILTERS));
      if (unknown !== undefined) return `unknown filter |${unknown}`;
      parts.push({ name, filters });
      i = close;
    } else if (c === "[") {
      if (parts !== top) return "[ ... ] can't be nested";
      flush();
      const optional: Part[] = [];
      top.push({ optional });
      parts = optional;
    } else if (c === "]") {
      if (parts === top) return "a ] has no [";
      flush();
      parts = top;
    } else if (c === "}") {
      return "a } has no {";
    } else {
      text += c;
    }
  }
  if (parts !== top) return "a [ has no ]";
  flush();
  return top;
}

/** What is wrong with a template, or null if it can be used */
export function templateError(template: string): string | null {
  const parsed = parse(template);
  return typeof parsed === "string" ? parsed : null;
}

/**
 * The link for a passage, or null when there is none: no template, a template that can't be
 * used, or one that needs a value the passage doesn't have (like `{book.usfm}` for a book
 * outside the 66)
 */
export function linkUrl(template: string, passage: Passage, book?: LinkBook): string | null {
  if (!template.trim()) return null;
  const parsed = parse(template);
  if (typeof parsed === "string") return null;
  const known = values(passage, book);
  const render = (parts: Part[]): string | null => {
    let out = "";
    for (const part of parts) {
      if ("text" in part) {
        out += part.text;
      } else if ("name" in part) {
        const value = known[part.name];
        if (value === undefined) return null;
        out += encodeURIComponent(part.filters.reduce((v, f) => FILTERS[f]!(v), value));
      } else {
        out += render(part.optional) ?? "";
      }
    }
    return out;
  };
  return render(parsed);
}

/** The site a template opens: a built-in site's name, or the address's host */
export function siteName(template: string): string {
  const site = SITES.find((s) => s.template === template.trim());
  if (site) return site.name;
  try {
    return new URL(template.replace(/[[\]{}]/g, "")).host || "the browser";
  } catch {
    return "the browser";
  }
}

/**
 * The link template for settings saved before templates, which chose a Literal Word
 * translation (`esv`, or "" for Literal Word's default)
 */
export function templateFromTranslation(translation: string): string {
  return translation
    ? `https://app.literalword.com/${translation}/{book.id}/{chapter}[/{verse}]`
    : SITES[0]!.template;
}
