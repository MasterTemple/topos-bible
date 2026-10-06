import { NO_FILTERS, type Filters, type Testament } from "./filters.ts";

/** A named search, saved in the settings */
export interface SavedQuery {
  name: string;
  /** The CLI's filter options, like `--nt -g "Pauline Epistles" -o "John 1"` */
  query: string;
}

/** What a query asks for: filters, and optionally a folder to search (a path, like the CLI's) */
export interface ParsedQuery {
  filters: Filters;
  folder: string | null;
  errors: string[];
}

type ListKey = Exclude<keyof Filters, "testaments" | "excludeTestaments">;

const LIST_OPTIONS: Record<string, ListKey> = {
  "-g": "genres",
  "--genre": "genres",
  "--exclude-genre": "excludeGenres",
  "-b": "books",
  "--book": "books",
  "--exclude-book": "excludeBooks",
  "-i": "inside",
  "--inside": "inside",
  "-o": "overlaps",
  "--overlaps": "overlaps",
  "--outside": "outside",
};

const TESTAMENT_OPTIONS: Record<string, "testaments" | "excludeTestaments"> = {
  "-t": "testaments",
  "--testament": "testaments",
  "--exclude-testament": "excludeTestaments",
};

/** Splits on whitespace, keeping "double" or 'single' quoted text together */
export function tokenize(text: string): string[] {
  const tokens: string[] = [];
  const pattern = /"([^"]*)"?|'([^']*)'?|([^\s"']+)/g;
  let current: string | null = null;
  let last = -1;
  for (const m of text.matchAll(pattern)) {
    const part = m[1] ?? m[2] ?? m[3] ?? "";
    // Pieces with no space between them are one token, like --book="Song of Solomon"
    if (current !== null && m.index === last) current += part;
    else {
      if (current !== null) tokens.push(current);
      current = part;
    }
    last = m.index + m[0].length;
  }
  if (current !== null) tokens.push(current);
  return tokens;
}

function testament(value: string): Testament | null {
  const v = value.trim().toLowerCase();
  if (["o", "ot", "old", "old testament"].includes(v)) return "old";
  if (["n", "nt", "new", "new testament"].includes(v)) return "new";
  return null;
}

/** Reads the CLI's filter options; anything else is reported in `errors` */
export function parseQuery(text: string): ParsedQuery {
  const filters: Filters = structuredClone(NO_FILTERS);
  const errors: string[] = [];
  let folder: string | null = null;
  const tokens = tokenize(text);
  for (let i = 0; i < tokens.length; i++) {
    let option = tokens[i];
    let value: string | undefined;
    const eq = option.indexOf("=");
    if (option.startsWith("--") && eq !== -1) {
      value = option.slice(eq + 1);
      option = option.slice(0, eq);
    }
    if (option === "--nt" || option === "--ot") {
      const t: Testament = option === "--nt" ? "new" : "old";
      if (!filters.testaments.includes(t)) filters.testaments.push(t);
      continue;
    }
    const list = LIST_OPTIONS[option];
    const testamentKey = TESTAMENT_OPTIONS[option];
    if (!list && !testamentKey) {
      if (option.startsWith("-")) errors.push(`Unknown option ${option}`);
      else if (folder === null) folder = option.replace(/^\.\/?/, "").replace(/\/+$/, "");
      else errors.push(`Only one folder can be searched ("${option}")`);
      continue;
    }
    value ??= tokens[++i];
    if (value === undefined) {
      errors.push(`${option} needs a value`);
      break;
    }
    if (list) filters[list].push(value);
    else {
      const t = testament(value);
      if (t === null) errors.push(`Unknown testament "${value}"`);
      else if (!filters[testamentKey].includes(t)) filters[testamentKey].push(t);
    }
  }
  return { filters, folder, errors };
}

function quote(value: string): string {
  return /^[\w:.,;\-–]+$/.test(value) ? value : `"${value.replace(/"/g, "")}"`;
}

/** Writes filters (and a folder) as CLI options, the inverse of {@link parseQuery} */
export function formatQuery(filters: Filters, folder: string | null = null): string {
  const parts: string[] = [];
  if (folder) parts.push(quote(folder));
  for (const t of filters.testaments) parts.push(t === "new" ? "--nt" : "--ot");
  for (const t of filters.excludeTestaments) parts.push("--exclude-testament", t);
  const lists: [ListKey, string][] = [
    ["genres", "-g"],
    ["excludeGenres", "--exclude-genre"],
    ["books", "-b"],
    ["excludeBooks", "--exclude-book"],
    ["inside", "-i"],
    ["overlaps", "-o"],
    ["outside", "--outside"],
  ];
  for (const [key, option] of lists) for (const value of filters[key]) parts.push(option, quote(value));
  return parts.join(" ");
}
