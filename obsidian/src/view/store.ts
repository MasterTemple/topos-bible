import { NO_FILTERS, type Filters } from "../core/filters.ts";
import type { SortOrder } from "../core/sort.ts";

export type Scope = "vault" | "file" | "folder";

export interface SearchState {
  filters: Filters;
  scope: Scope;
  folder: string;
  sort: SortOrder;
  groupBy: "file" | "book";
}

/** The sidebar's state, kept on the plugin so commands can set filters and the view survives reloads */
export class SearchStore {
  private state: SearchState;
  private readonly listeners = new Set<() => void>();

  constructor(initial: Partial<SearchState> = {}) {
    this.state = { filters: NO_FILTERS, scope: "vault", folder: "", sort: "file", groupBy: "file", ...initial };
  }

  get = (): SearchState => this.state;

  set(update: Partial<SearchState>): void {
    this.state = { ...this.state, ...update };
    for (const listener of this.listeners) listener();
  }

  subscribe = (listener: () => void): (() => void) => {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  };
}
