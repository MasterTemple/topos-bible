import { RangeSetBuilder, StateEffect } from "@codemirror/state";
import {
  Decoration,
  EditorView,
  ViewPlugin,
  type DecorationSet,
  type PluginValue,
  type ViewUpdate,
} from "@codemirror/view";
import { OffsetUnit } from "topos-bible";
import type ToposPlugin from "../main.ts";

/** Rebuilds the references in an editor, after the settings change what they link to */
export const refreshReferences = StateEffect.define<null>();

/**
 * Highlights references in the visible part of the editor, and opens their links on
 * click (Ctrl/Cmd-click by default, so a plain click still places the cursor). Only
 * references with a link are highlighted, so with links turned off there are none.
 */
export function referenceDecorations(plugin: ToposPlugin) {
  class References implements PluginValue {
    decorations: DecorationSet;

    constructor(view: EditorView) {
      this.decorations = this.build(view);
    }

    update(update: ViewUpdate): void {
      const refresh = update.transactions.some((tr) => tr.effects.some((e) => e.is(refreshReferences)));
      if (refresh || update.docChanged || update.viewportChanged) this.decorations = this.build(update.view);
    }

    build(view: EditorView): DecorationSet {
      const builder = new RangeSetBuilder<Decoration>();
      if (!plugin.settings.linkInEditor || !plugin.settings.linkTemplate.trim()) return builder.finish();
      const site = plugin.linkSite();
      // Visible ranges widened to whole lines can overlap, and the builder needs sorted ranges
      let lastEnd = -1;
      for (const { from, to } of view.visibleRanges) {
        // Whole lines, so references are not cut at the edge of the viewport
        const start = view.state.doc.lineAt(from).from;
        const end = view.state.doc.lineAt(to).to;
        const text = view.state.doc.sliceString(start, end);
        for (const m of plugin.topos.search(text, OffsetUnit.Utf16)) {
          const url = plugin.referenceUrl(m.passage);
          if (!url) continue;
          const mark = Decoration.mark({
            class: "topos-reference",
            attributes: { title: `${m.passage.reference}, opens in ${site}`, "data-topos-url": url },
          });
          if (start + m.start < lastEnd) continue;
          builder.add(start + m.start, start + m.end, mark);
          lastEnd = start + m.end;
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
