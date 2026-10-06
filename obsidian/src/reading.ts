import type { MarkdownPostProcessor } from "obsidian";
import { OffsetUnit } from "topos-bible";
import type ToposPlugin from "./main.ts";

/** Elements whose text is never linked */
const SKIP = new Set(["A", "CODE", "PRE", "SCRIPT", "STYLE"]);

/** Turns references in reading view into links (from the link template) */
export function linkReferences(plugin: ToposPlugin): MarkdownPostProcessor {
  return (el) => {
    if (!plugin.settings.linkInReading) return;
    const walker = document.createTreeWalker(el, NodeFilter.SHOW_TEXT, {
      acceptNode(node) {
        for (let parent = node.parentElement; parent && parent !== el; parent = parent.parentElement) {
          if (SKIP.has(parent.tagName)) return NodeFilter.FILTER_REJECT;
        }
        return NodeFilter.FILTER_ACCEPT;
      },
    });
    const nodes: Text[] = [];
    while (walker.nextNode()) nodes.push(walker.currentNode as Text);

    for (const node of nodes) {
      const text = node.data;
      const matches = plugin.topos.search(text, OffsetUnit.Utf16);
      if (matches.length === 0) continue;
      const fragment = document.createDocumentFragment();
      let last = 0;
      for (const m of matches) {
        const url = plugin.referenceUrl(m.passage);
        if (!url) continue;
        fragment.append(text.slice(last, m.start));
        const link = createEl("a", {
          cls: "topos-reference external-link",
          text: text.slice(m.start, m.end),
          href: url,
          attr: { title: `${m.passage.reference} (${m.passage.osis})`, target: "_blank", rel: "noopener" },
        });
        fragment.append(link);
        last = m.end;
      }
      fragment.append(text.slice(last));
      node.replaceWith(fragment);
    }
  };
}
