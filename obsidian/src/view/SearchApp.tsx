import { useEffect, useMemo, useState, useSyncExternalStore, type ReactNode } from "react";
import { compileFilters, isEmpty, keep, NO_FILTERS, type Filters, type Testament } from "../core/filters.ts";
import { literalWordUrl } from "../core/literalWord.ts";
import { styled } from "../core/references.ts";
import type { Hit } from "../core/search.ts";
import { groupHits, sortHits, type SortOrder } from "../core/sort.ts";
import type ToposPlugin from "../main.ts";
import { bookStyle, type StyleName } from "../core/settings.ts";
import { Chips, NameInput, ReferenceInput } from "./inputs.tsx";
import type { Scope } from "./store.ts";

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

  const setFilters = (update: Partial<Filters>) => {
    plugin.search.set({ filters: { ...state.filters, ...update } });
    setShown(PAGE);
  };
  const addTo = (key: ListKey) => (value: string) =>
    setFilters({ [key]: [...state.filters[key], value] } as Partial<Filters>);
  const removeFrom = (key: ListKey) => (i: number) =>
    setFilters({ [key]: state.filters[key].filter((_, j) => j !== i) } as Partial<Filters>);

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
          value={styleName}
          onChange={(e) => setStyleName(e.target.value as StyleName)}
          aria-label="Reference style"
        >
          <option value="name">John 3:16</option>
          <option value="abbreviation">Jn 3:16</option>
          <option value="osis">John.3.16</option>
        </select>
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
          Filters
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
        <FilterRow label="Inside" hint="entirely within">
          <Chips values={state.filters.inside} onRemove={removeFrom("inside")} />
          <ReferenceInput topos={topos} style={style} placeholder="Romans 8" onSubmit={addTo("inside")} />
        </FilterRow>
        <FilterRow label="Overlapping" hint="shares a verse with">
          <Chips values={state.filters.overlaps} onRemove={removeFrom("overlaps")} />
          <ReferenceInput topos={topos} style={style} placeholder="John 3:16-21" onSubmit={addTo("overlaps")} />
        </FilterRow>
        <FilterRow label="Outside" hint="shares no verse with">
          <Chips values={state.filters.outside} onRemove={removeFrom("outside")} exclude />
          <ReferenceInput topos={topos} style={style} placeholder="Psalm 23" onSubmit={addTo("outside")} />
        </FilterRow>
        {filter.errors.length > 0 && (
          <ul className="topos-errors">
            {filter.errors.map((error) => (
              <li key={error}>{error}</li>
            ))}
          </ul>
        )}
      </details>

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
                    reference={styled(topos, hit.passage, style)}
                    showPath={state.groupBy === "book"}
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

function FilterRow({ label, hint, children }: { label: string; hint?: string; children: ReactNode }) {
  return (
    <div className="topos-filter-row">
      <label title={hint}>{label}</label>
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

function HitRow({
  plugin,
  hit,
  reference,
  showPath,
}: {
  plugin: ToposPlugin;
  hit: Hit;
  reference: string;
  showPath: boolean;
}) {
  const from = hit.column - 1;
  const to = Math.min(hit.lineText.length, from + (hit.end - hit.start));
  const url = literalWordUrl(hit.passage, plugin.settings.translation);
  return (
    <div className="topos-hit" onClick={() => void plugin.openHit(hit)}>
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
      <div className="topos-hit-context">
        {hit.lineText.slice(0, from).trimStart()}
        <mark>{hit.lineText.slice(from, to)}</mark>
        {hit.lineText.slice(to)}
      </div>
    </div>
  );
}
