import { RangeSetBuilder } from "@codemirror/state";
import {
  Decoration,
  EditorView,
  ViewPlugin,
  type DecorationSet,
  type PluginValue,
  type ViewUpdate,
} from "@codemirror/view";
import { OffsetUnit } from "topos-bible";
import { literalWordUrl } from "../core/literalWord.ts";
import type ToposPlugin from "../main.ts";

/**
 * Underlines references in the visible part of the editor, and opens them in Literal Word on
 * click (Ctrl/Cmd-click by default, so a plain click still places the cursor).
 */
export function referenceDecorations(plugin: ToposPlugin) {
  class References implements PluginValue {
    decorations: DecorationSet;

    constructor(view: EditorView) {
      this.decorations = this.build(view);
    }

    update(update: ViewUpdate): void {
      if (update.docChanged || update.viewportChanged) this.decorations = this.build(update.view);
    }

    build(view: EditorView): DecorationSet {
      const builder = new RangeSetBuilder<Decoration>();
      if (!plugin.settings.linkInEditor) return builder.finish();
      for (const { from, to } of view.visibleRanges) {
        // Whole lines, so references are not cut at the edge of the viewport
        const start = view.state.doc.lineAt(from).from;
        const end = view.state.doc.lineAt(to).to;
        const text = view.state.doc.sliceString(start, end);
        for (const m of plugin.topos.search(text, OffsetUnit.Utf16)) {
          const url = literalWordUrl(m.passage, plugin.settings.translation);
          const mark = Decoration.mark({
            class: "topos-reference",
            attributes: {
              title: `${m.passage.reference} (${m.passage.osis})`,
              ...(url ? { "data-topos-url": url } : {}),
            },
          });
          builder.add(start + m.start, start + m.end, mark);
        }
      }
      return builder.finish();
    }
  }

  return [
    ViewPlugin.fromClass(References, { decorations: (value) => value.decorations }),
    EditorView.domEventHandlers({
      click(event: MouseEvent) {
        const target = (event.target as HTMLElement | null)?.closest<HTMLElement>(
          "[data-topos-url]",
        );
        if (!target || !plugin.settings.linkInEditor) return false;
        if (plugin.settings.clickNeedsModifier && !(event.ctrlKey || event.metaKey)) return false;
        event.preventDefault();
        window.open(target.dataset.toposUrl, "_blank");
        return true;
      },
    }),
  ];
}
