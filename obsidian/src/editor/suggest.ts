import {
  Editor,
  EditorSuggest,
  type EditorPosition,
  type EditorSuggestContext,
  type EditorSuggestTriggerInfo,
} from "obsidian";
import { CompletionKind, type Completion } from "topos-bible";
import { completionsBefore } from "../core/completions.ts";
import type ToposPlugin from "../main.ts";
import { bookStyle } from "../settings.ts";

const KIND_LABELS: Record<number, string> = {
  [CompletionKind.Book]: "book",
  [CompletionKind.Chapter]: "chapter",
  [CompletionKind.Verse]: "verse",
};

/** Suggests books, chapters, and verses for the reference being typed in the editor */
export class ReferenceSuggest extends EditorSuggest<Completion> {
  private readonly plugin: ToposPlugin;

  constructor(plugin: ToposPlugin) {
    super(plugin.app);
    this.plugin = plugin;
    this.setInstructions([
      { command: "↑↓", purpose: "to navigate" },
      { command: "↵", purpose: "to complete" },
      { command: "esc", purpose: "to dismiss" },
    ]);
  }

  private completions(editor: Editor, cursor: EditorPosition): Completion[] {
    const { settings, topos } = this.plugin;
    const before = editor.getLine(cursor.line).slice(0, cursor.ch);
    return completionsBefore(
      topos,
      before,
      bookStyle(settings.style),
      settings.suggestionLimit,
      settings.bookCompletion,
      { needsNumber: true, joinAdjacent: settings.joinAdjacent },
    );
  }

  onTrigger(cursor: EditorPosition, editor: Editor): EditorSuggestTriggerInfo | null {
    if (!this.plugin.settings.autocomplete) return null;
    const completions = this.completions(editor, cursor);
    if (completions.length === 0) return null;
    const start = Math.min(...completions.map((c) => c.start));
    return {
      start: { line: cursor.line, ch: start },
      end: cursor,
      query: editor.getLine(cursor.line).slice(start, cursor.ch),
    };
  }

  getSuggestions(context: EditorSuggestContext): Completion[] {
    return this.completions(context.editor, context.end);
  }

  renderSuggestion(completion: Completion, el: HTMLElement): void {
    el.addClass("topos-suggestion");
    el.createSpan({ text: completion.label });
    el.createSpan({ cls: "topos-suggestion-kind", text: KIND_LABELS[completion.kind] });
  }

  selectSuggestion(completion: Completion): void {
    const context = this.context;
    if (!context) return;
    const line = context.end.line;
    context.editor.replaceRange(
      completion.text,
      { line, ch: completion.start },
      { line, ch: completion.end },
    );
    context.editor.setCursor({ line, ch: completion.start + completion.text.length });
  }
}
