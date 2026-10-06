import { useEffect, useMemo, useState, useSyncExternalStore, type ReactNode } from "react";
import { compileFilters, isEmpty, keep, NO_FILTERS, type Filters, type Testament } from "../core/filters.ts";
import { literalWordUrl } from "../core/literalWord.ts";
import { formatQuery, parseQuery } from "../core/query.ts";
import { written } from "../core/format.ts";
import type { Hit } from "../core/search.ts";
import { groupHits, sortHits, type SortOrder } from "../core/sort.ts";
import type ToposPlugin from "../main.ts";
import { bookStyle, type StyleName } from "../core/settings.ts";
import { Chips, NameInput, ReferenceInput } from "./inputs.tsx";
import type { Scope } from "./store.ts";

/** The saved-search list's entry that clears the filters */
const CLEAR = "topos:clear-filters";

/** How many results render before "Show more", so huge vaults stay responsive */
const PAGE = 300;

type ListKey = Exclude<keyof Filters, "testaments" | "excludeTestaments">;

export function SearchApp({ plugin }: { plugin: ToposPlugin }) {
  const { topos } = plugin;
  const state = useSyncExternalStore(plugin.search.subscribe, plugin.search.get, plugin.search.get);
  const version = () => plugin.indexVersion;
  const indexVersion = useSyncExternalStore(plugin.subscribeIndex, version, version);
  const [styleName, setStyleName] = useState<StyleName>(plugin.settings.style);
  const [activePath, setActivePath] = useState(plugin.app.workspace.getActiveFile()?.path ?? "");
  const [collapsed, setCollapsed] = useState<Set<string>>(new Set());
  const [shown, setShown] = useState(PAGE);
  const [filtersOpen, setFiltersOpen] = useState(!isEmpty(state.filters));
  /** The name being typed to save the current search, or null */
  const [saving, setSaving] = useState<string | null>(null);
  const style = bookStyle(styleName);

  useEffect(() => {
    const ref = plugin.app.workspace.on("file-open", (file) => {
      if (file) setActivePath(file.path);
    });
    return () => plugin.app.workspace.offref(ref);
  }, [plugin]);

  const filter = useMemo(() => compileFilters(topos, state.filters), [topos, state.filters]);
  const hits = useMemo(() => {
    const folder = state.folder.replace(/\/+$/, "");
    const inScope =
      state.scope === "file"
        ? plugin.index.get(activePath)
        : plugin.index
            .all()
            .filter((hit) => state.scope === "vault" || !folder || hit.path.startsWith(`${folder}/`));
    return sortHits(
      inScope.filter((hit) => keep(topos, filter, hit.passage)),
      state.sort,
    );
    // indexVersion: recompute when files change
  }, [topos, plugin, filter, state.scope, state.folder, state.sort, activePath, indexVersion]);

  const books = useMemo(() => topos.books(), [topos]);
  const genres = useMemo(() => topos.genres(), [topos]);
  const bookName = (id: number) => books.find((b) => b.id === id)?.name ?? `Book ${id}`;
  const groups = useMemo(
    () => groupHits(hits.slice(0, shown), state.groupBy, bookName),
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [hits, shown, state.groupBy, books],
  );
  const fileCount = useMemo(() => new Set(hits.map((h) => h.path)).size, [hits]);
  const shownPaths = useMemo(
    () => [...new Set(groups.flatMap((g) => g.hits.map((h) => h.path)))],
    [groups],
  );
  const fileLines = useFileLines(plugin, state.context > 0 ? shownPaths : [], indexVersion);

  const setFilters = (update: Partial<Filters>) => {
    plugin.search.set({ filters: { ...state.filters, ...update } });
    setShown(PAGE);
  };
  const addTo = (key: ListKey) => (value: string) =>
    setFilters({ [key]: [...state.filters[key], value] } as Partial<Filters>);
  const removeFrom = (key: ListKey) => (i: number) =>
    setFilters({ [key]: state.filters[key].filter((_, j) => j !== i) } as Partial<Filters>);

  const activeFilters = Object.values(state.filters).reduce((n, values) => n + values.length, 0);
  const query = formatQuery(state.filters, state.scope === "folder" ? state.folder.replace(/\/+$/, "") || null : null);
  const savedQueries = plugin.settings.queries;
  const sameQuery = (text: string) => {
    const parsed = parseQuery(text);
    return parsed.errors.length === 0 && formatQuery(parsed.filters, parsed.folder) === query;
  };
  const active = savedQueries.find((saved) => sameQuery(saved.query)) ?? null;
  const save = () => {
    if (saving?.trim()) void plugin.saveQuery({ name: saving, query });
    setSaving(null);
  };

  const bookOptions = books.map((b) => ({ name: b.name, aliases: [b.abbreviation, b.osis] }));
  const genreOptions = genres.map((g) => ({ name: g.name, aliases: [] }));

  return (
    <div className="topos-search">
      <div className="topos-toolbar">
        <select
          value={state.scope}
          onChange={(e) => plugin.search.set({ scope: e.target.value as Scope })}
          aria-label="Where to search"
        >
          <option value="vault">Whole vault</option>
          <option value="file">Current note</option>
          <option value="folder">Folder</option>
        </select>
        <select
          value={state.sort}
          onChange={(e) => plugin.search.set({ sort: e.target.value as SortOrder })}
          aria-label="Sort"
        >
          <option value="file">Order in notes</option>
          <option value="bible">Order in the Bible</option>
        </select>
        <select
          value={state.groupBy}
          onChange={(e) => plugin.search.set({ groupBy: e.target.value as "file" | "book" })}
          aria-label="Group by"
        >
          <option value="file">Group by note</option>
          <option value="book">Group by book</option>
        </select>
        <select
          value={state.context}
          onChange={(e) => plugin.search.set({ context: Number(e.target.value) })}
          aria-label="Context lines"
          title="Lines of context around each result"
        >
          {[0, 1, 2, 3, 5, 10].map((n) => (
            <option key={n} value={n}>
              {n === 0 ? "No context" : `±${n} line${n === 1 ? "" : "s"}`}
            </option>
          ))}
        </select>
        <select
          value={styleName}
          onChange={(e) => setStyleName(e.target.value as StyleName)}
          aria-label="Reference style"
        >
          <option value="name">John 3:16</option>
          <option value="abbreviation">Jn 3:16</option>
          <option value="osis">John.3.16</option>
        </select>
      </div>
      <div className="topos-saved">
        {saving === null ? (
          <>
            <select
              value={active?.name ?? ""}
              onChange={(e) => {
                if (e.target.value === CLEAR) {
                  plugin.search.set({ filters: NO_FILTERS, scope: "vault", folder: "" });
                  setShown(PAGE);
                  return;
                }
                const saved = savedQueries.find((q) => q.name === e.target.value);
                if (!saved) return;
                plugin.applyQuery(saved);
                setFiltersOpen(true);
                setShown(PAGE);
              }}
              aria-label="Saved searches"
            >
              {/* The prompt shows when no saved search matches; picking it does nothing */}
              <option value="" disabled hidden>
                {savedQueries.length === 0 ? "No saved searches" : "Saved searches…"}
              </option>
              <option value={CLEAR} disabled={activeFilters === 0 && state.scope === "vault"}>
                New search (clear filters)
              </option>
              {savedQueries.map((saved) => (
                <option key={saved.name} value={saved.name} title={saved.query}>
                  {saved.name}
                </option>
              ))}
            </select>
            <button
              onClick={() => setSaving(active?.name ?? "")}
              disabled={activeFilters === 0 && state.scope !== "folder"}
              title="Save these filters under a name"
            >
              Save
            </button>
            {active && (
              <button onClick={() => void plugin.deleteQuery(active.name)} title={`Delete "${active.name}"`}>
                Delete
              </button>
            )}
          </>
        ) : (
          <>
            <input
              type="text"
              autoFocus
              value={saving}
              placeholder="Name this search"
              onChange={(e) => setSaving(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter") save();
                if (e.key === "Escape") setSaving(null);
              }}
            />
            <button className="mod-cta" onClick={save} disabled={!saving.trim()}>
              Save
            </button>
            <button onClick={() => setSaving(null)}>Cancel</button>
          </>
        )}
      </div>

      {state.scope === "folder" && (
        <input
          className="topos-folder"
          type="text"
          value={state.folder}
          placeholder="Folder, like Sermons/2025"
          onChange={(e) => plugin.search.set({ folder: e.target.value })}
        />
      )}

      <details
        className="topos-filters"
        open={filtersOpen}
        onToggle={(e) => setFiltersOpen((e.target as HTMLDetailsElement).open)}
      >
        <summary>
          <span className="topos-filters-title">
            Filters
            {activeFilters > 0 && <span className="topos-badge">{activeFilters}</span>}
          </span>
          {filter.books && !filter.conflict && (
            <span className="topos-filters-books">
              {`${filter.books.size} book${filter.books.size === 1 ? "" : "s"}`}
            </span>
          )}
          {!isEmpty(state.filters) && (
            <button
              className="topos-clear"
              onClick={(e) => {
                e.preventDefault();
                plugin.search.set({ filters: NO_FILTERS });
              }}
            >
              Clear
            </button>
          )}
        </summary>
        <div className="topos-filter-row">
          <label>Testament</label>
          <div className="topos-testaments">
            {(["old", "new"] as Testament[]).map((t) => (
              <TestamentToggle key={t} testament={t} filters={state.filters} onChange={setFilters} />
            ))}
          </div>
        </div>
        <FilterRow label="Genres">
          <Chips values={state.filters.genres} onRemove={removeFrom("genres")} />
          <NameInput options={genreOptions} placeholder="Gospels" onSubmit={addTo("genres")} />
        </FilterRow>
        <FilterRow label="Not genres">
          <Chips values={state.filters.excludeGenres} onRemove={removeFrom("excludeGenres")} exclude />
          <NameInput options={genreOptions} placeholder="Pauline Epistles" onSubmit={addTo("excludeGenres")} />
        </FilterRow>
        <FilterRow label="Books">
          <Chips values={state.filters.books} onRemove={removeFrom("books")} />
          <NameInput options={bookOptions} placeholder="John" onSubmit={addTo("books")} />
        </FilterRow>
        <FilterRow label="Not books">
          <Chips values={state.filters.excludeBooks} onRemove={removeFrom("excludeBooks")} exclude />
          <NameInput options={bookOptions} placeholder="Psalms" onSubmit={addTo("excludeBooks")} />
        </FilterRow>
        {PASSAGE_ROWS.map((row) => (
          <FilterRow key={row.key} label={row.label} hint={row.hint}>
            <Chips
              values={state.filters[row.key]}
              onRemove={removeFrom(row.key)}
              exclude={row.key === "excludeOverlap"}
            />
            <ReferenceInput
              topos={topos}
              style={style}
              placeholder={row.placeholder}
              onSubmit={addTo(row.key)}
              format={plugin.settings.format}
            />
          </FilterRow>
        ))}
        {filter.errors.length > 0 && (
          <ul className="topos-errors">
            {filter.errors.map((error) => (
              <li key={error}>{error}</li>
            ))}
          </ul>
        )}
        {query && (
          <div className="topos-query" title="These filters as topos CLI options, for saved searches or the terminal">
            <code>{query}</code>
            <button
              className="topos-copy"
              onClick={() => {
                void navigator.clipboard.writeText(query);
              }}
              title="Copy"
            >
              Copy
            </button>
          </div>
        )}
      </details>

      {filter.conflict && <div className="topos-conflict">⚠ {filter.conflict}</div>}

      <div className="topos-summary">
        {plugin.indexing
          ? "Indexing…"
          : `${hits.length} reference${hits.length === 1 ? "" : "s"} in ${fileCount} note${fileCount === 1 ? "" : "s"}`}
      </div>

      <div className="topos-results">
        {groups.map((group) => {
          const isCollapsed = collapsed.has(group.key);
          return (
            <div key={group.key} className="topos-group">
              <div
                className="topos-group-header"
                onClick={() => {
                  const next = new Set(collapsed);
                  if (isCollapsed) next.delete(group.key);
                  else next.add(group.key);
                  setCollapsed(next);
                }}
              >
                <span className={`topos-caret${isCollapsed ? " is-collapsed" : ""}`}>▾</span>
                <span className="topos-group-name">{group.key}</span>
                <span className="topos-count">{group.hits.length}</span>
              </div>
              {!isCollapsed &&
                group.hits.map((hit) => (
                  <HitRow
                    key={`${hit.path}:${hit.start}`}
                    plugin={plugin}
                    hit={hit}
                    reference={written(topos, hit.passage, style, plugin.settings.format)}
                    showPath={state.groupBy === "book"}
                    context={state.context}
                    lines={fileLines.get(hit.path)}
                  />
                ))}
            </div>
          );
        })}
        {hits.length > shown && (
          <button className="topos-more" onClick={() => setShown(shown + PAGE)}>
            Show more ({hits.length - shown} left)
          </button>
        )}
      </div>
    </div>
  );
}

/** The passage filters, like the CLI's */
const PASSAGE_ROWS: { key: ListKey; label: string; hint: string; placeholder: string }[] = [
  { key: "inside", label: "Inside", hint: "entirely within", placeholder: "Romans 8" },
  { key: "explicitOverlap", label: "Names", hint: "names its verses", placeholder: "John 3:16-21" },
  { key: "anyOverlap", label: "Overlaps", hint: "any shared verse", placeholder: "John 3:16-21" },
  { key: "exactOverlap", label: "Exactly", hint: "same verses", placeholder: "John 3:16" },
  { key: "excludeOverlap", label: "Not", hint: "no shared verse", placeholder: "Psalm 23" },
];

function FilterRow({ label, hint, children }: { label: string; hint?: string; children: ReactNode }) {
  return (
    <div className="topos-filter-row">
      <label title={hint}>
        {label}
        {hint && <span className="topos-filter-hint">{hint}</span>}
      </label>
      <div className="topos-filter-values">{children}</div>
    </div>
  );
}

/** Cycles a testament through any, included, and excluded */
function TestamentToggle({
  testament,
  filters,
  onChange,
}: {
  testament: Testament;
  filters: Filters;
  onChange: (update: Partial<Filters>) => void;
}) {
  const included = filters.testaments.includes(testament);
  const excluded = filters.excludeTestaments.includes(testament);
  const label = testament === "old" ? "OT" : "NT";
  const without = (list: Testament[]) => list.filter((t) => t !== testament);
  return (
    <button
      className={`topos-toggle${included ? " is-include" : excluded ? " is-exclude" : ""}`}
      title={included ? "Included (click to exclude)" : excluded ? "Excluded (click to clear)" : "Click to include"}
      onClick={() =>
        onChange(
          included
            ? { testaments: without(filters.testaments), excludeTestaments: [...filters.excludeTestaments, testament] }
            : excluded
              ? { excludeTestaments: without(filters.excludeTestaments) }
              : { testaments: [...filters.testaments, testament] },
        )
      }
    >
      {excluded ? `not ${label}` : label}
    </button>
  );
}

/**
 * The lines of the notes being shown, for context (read on demand, so it works the same whichever
 * engine indexed them, and costs nothing with context off)
 */
function useFileLines(plugin: ToposPlugin, paths: string[], indexVersion: number): Map<string, string[]> {
  const [lines, setLines] = useState(() => new Map<string, string[]>());
  const key = paths.join("\n");
  useEffect(() => {
    if (paths.length === 0) return;
    let cancelled = false;
    void (async () => {
      const next = new Map<string, string[]>();
      for (const path of paths) {
        const file = plugin.app.vault.getFileByPath?.(path);
        if (!file) continue;
        next.set(path, (await plugin.app.vault.cachedRead(file)).split(/\r?\n/));
      }
      if (!cancelled) setLines(next);
    })();
    return () => {
      cancelled = true;
    };
    // paths is summarized by key; indexVersion re-reads notes that changed
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [plugin, key, indexVersion]);
  return lines;
}

function HitRow({
  plugin,
  hit,
  reference,
  showPath,
  context,
  lines,
}: {
  plugin: ToposPlugin;
  hit: Hit;
  reference: string;
  showPath: boolean;
  context: number;
  lines: string[] | undefined;
}) {
  const from = hit.column - 1;
  const to = Math.min(hit.lineText.length, from + (hit.end - hit.start));
  const url = literalWordUrl(hit.passage, plugin.settings.translation);
  return (
    <div className={`topos-hit${context > 0 ? " has-context" : ""}`} onClick={() => void plugin.openHit(hit)}>
      <div className="topos-hit-head">
        <span className="topos-hit-reference">{reference}</span>
        <span className="topos-hit-location">
          {showPath ? `${hit.path}:` : ""}
          {hit.line}
        </span>
        {url && (
          <button
            className="topos-lw"
            title="Open in Literal Word"
            onClick={(e) => {
              e.stopPropagation();
              window.open(url, "_blank");
            }}
          >
            ↗
          </button>
        )}
      </div>
      {context > 0 && lines && <ContextLines lines={lines} from={hit.line - 1 - context} to={hit.line - 2} />}
      <div className="topos-hit-context">
        {context > 0 ? hit.lineText.slice(0, from) : hit.lineText.slice(0, from).trimStart()}
        <mark>{hit.lineText.slice(from, to)}</mark>
        {hit.lineText.slice(to)}
      </div>
      {context > 0 && lines && <ContextLines lines={lines} from={hit.line} to={hit.line - 1 + context} />}
    </div>
  );
}

/** Lines `from` to `to` (0-based, inclusive) of a note, for context around a result */
function ContextLines({ lines, from, to }: { lines: string[]; from: number; to: number }) {
  const shown = [];
  for (let i = Math.max(0, from); i <= Math.min(lines.length - 1, to); i++) {
    shown.push(
      <div key={i} className="topos-context-line">
        {lines[i] || "\u00a0"}
      </div>,
    );
  }
  return <>{shown}</>;
}
