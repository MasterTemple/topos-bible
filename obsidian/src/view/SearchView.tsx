import { ItemView, type WorkspaceLeaf } from "obsidian";
import { StrictMode } from "react";
import { createRoot, type Root } from "react-dom/client";
import type ToposPlugin from "../main.ts";
import { ErrorBoundary } from "./ErrorBoundary.tsx";
import { SearchApp } from "./SearchApp.tsx";

export const SEARCH_VIEW = "topos-bible-search";

/** The verse search sidebar */
export class SearchView extends ItemView {
  private readonly plugin: ToposPlugin;
  private root: Root | null = null;

  constructor(leaf: WorkspaceLeaf, plugin: ToposPlugin) {
    super(leaf);
    this.plugin = plugin;
  }

  getViewType(): string {
    return SEARCH_VIEW;
  }

  getDisplayText(): string {
    return "Verse search";
  }

  getIcon(): string {
    return "book-open";
  }

  async onOpen(): Promise<void> {
    this.root = createRoot(this.contentEl);
    this.root.render(
      <StrictMode>
        <ErrorBoundary>
          <SearchApp plugin={this.plugin} />
        </ErrorBoundary>
      </StrictMode>,
    );
  }

  async onClose(): Promise<void> {
    this.root?.unmount();
    this.root = null;
  }
}
