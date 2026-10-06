import { App, Editor, SuggestModal } from "obsidian";
import { CompletionKind, type Completion } from "topos-bible";
import { applyCompletion, completionsBefore } from "./core/completions.ts";
import { compileFilters, keep, NO_FILTERS } from "./core/filters.ts";
import type { SavedQuery } from "./core/query.ts";
import { styled } from "./core/references.ts";
import type { Hit } from "./core/search.ts";
import { sortHits } from "./core/sort.ts";
import type ToposPlugin from "./main.ts";
import { bookStyle } from "./settings.ts";

/** A completion, or the typed reference itself when it is already complete */
type Choice = { kind: "completion"; completion: Completion } | { kind: "reference"; text: string };

/** Type a reference with autocomplete, then insert it at the cursor */
export class InsertReferenceModal extends SuggestModal<Choice> {
  private readonly plugin: ToposPlugin;
  private readonly editor: Editor;

  constructor(app: App, plugin: ToposPlugin, editor: Editor) {
    super(app);
    this.plugin = plugin;
    this.editor = editor;
    this.setPlaceholder("Type a reference, like John 3:16");
    this.setInstructions([
      { command: "↵", purpose: "complete, or insert a finished reference" },
      { command: "esc", purpose: "to dismiss" },
    ]);
  }

  getSuggestions(query: string): Choice[] {
    const { settings, topos } = this.plugin;
    const style = bookStyle(settings.style);
    const choices: Choice[] = [];
    const passage = query.trim() ? topos.parse(query, style) : null;
    if (passage) choices.push({ kind: "reference", text: styled(topos, passage, style) });
    const completions = completionsBefore(topos, query, style, settings.suggestionLimit, "always");
    choices.push(...completions.map((completion) => ({ kind: "completion" as const, completion })));
    return choices;
  }

  renderSuggestion(choice: Choice, el: HTMLElement): void {
    el.addClass("topos-suggestion");
    if (choice.kind === "reference") {
      el.createSpan({ text: choice.text });
      el.createSpan({ cls: "topos-suggestion-kind", text: "insert" });
    } else {
      el.createSpan({ text: choice.completion.label });
      const kind = ["book", "chapter", "verse"][choice.completion.kind];
      el.createSpan({ cls: "topos-suggestion-kind", text: kind });
    }
  }

  /** Books and chapters keep the dialog open so the reference can be refined */
  selectSuggestion(choice: Choice, evt: MouseEvent | KeyboardEvent): void {
    if (choice.kind === "completion" && choice.completion.kind !== CompletionKind.Verse) {
      this.inputEl.value = applyCompletion(this.inputEl.value, choice.completion);
      this.inputEl.dispatchEvent(new Event("input"));
      return;
    }
    super.selectSuggestion(choice, evt);
  }

  onChooseSuggestion(choice: Choice): void {
    const text =
      choice.kind === "reference" ? choice.text : applyCompletion(this.inputEl.value, choice.completion);
    this.editor.replaceSelection(text);
  }
}

/** Type a passage and jump to any reference in the vault that overlaps it */
export class GoToReferenceModal extends SuggestModal<Hit> {
  private readonly plugin: ToposPlugin;

  constructor(app: App, plugin: ToposPlugin) {
    super(app);
    this.plugin = plugin;
    this.limit = 200;
    this.setPlaceholder("Type a passage, like Romans 8");
  }

  getSuggestions(query: string): Hit[] {
    const { topos, index } = this.plugin;
    if (!query.trim() || !topos.parse(query, 0)) return [];
    const filter = compileFilters(topos, { ...NO_FILTERS, anyOverlap: [query] });
    return sortHits(
      index.all().filter((hit) => keep(topos, filter, hit.passage)),
      "bible",
    );
  }

  renderSuggestion(hit: Hit, el: HTMLElement): void {
    const style = bookStyle(this.plugin.settings.style);
    el.addClass("topos-hit-suggestion");
    el.createDiv({ cls: "topos-hit-reference", text: styled(this.plugin.topos, hit.passage, style) });
    el.createDiv({ cls: "topos-hit-location", text: `${hit.path}:${hit.line}` });
    el.createDiv({ cls: "topos-hit-context", text: hit.lineText.trim() });
  }

  onChooseSuggestion(hit: Hit): void {
    void this.plugin.openHit(hit);
  }
}

/** Pick a saved search to show in the sidebar */
export class SavedQueryModal extends SuggestModal<SavedQuery> {
  private readonly plugin: ToposPlugin;

  constructor(app: App, plugin: ToposPlugin) {
    super(app);
    this.plugin = plugin;
    this.setPlaceholder("Saved search");
  }

  getSuggestions(query: string): SavedQuery[] {
    const q = query.trim().toLowerCase();
    return this.plugin.settings.queries.filter(
      (saved) => saved.name.toLowerCase().includes(q) || saved.query.toLowerCase().includes(q),
    );
  }

  renderSuggestion(saved: SavedQuery, el: HTMLElement): void {
    el.createDiv({ text: saved.name });
    el.createEl("small", { text: saved.query || "(no filters)", cls: "topos-query-text" });
  }

  onChooseSuggestion(saved: SavedQuery): void {
    void this.plugin.openQuery(saved);
  }
}
