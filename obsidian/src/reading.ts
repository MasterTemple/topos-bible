import type { MarkdownPostProcessor } from "obsidian";
import { OffsetUnit } from "topos-bible";
import type ToposPlugin from "./main.ts";

/** The class of references this file links (the editor's own highlights are CodeMirror's) */
const RENDERED = "topos-rendered";

/** Elements whose text is never linked */
const SKIP = new Set(["A", "CODE", "PRE", "SCRIPT", "STYLE"]);

/**
 * Rendered Markdown inside a live preview editor: callouts, tables, and embedded notes. The
 * editor keeps these until their text changes, so a new link setting relinks them in place
 */
const RENDERED_IN_EDITOR = ".cm-content .cm-embed-block, .cm-content .markdown-embed";

/**
 * Highlights references in reading view, opening their links (from the link template) on
 * Ctrl/Cmd-click like the editor. They are not links, so they don't look like one
 */
export function linkReferences(plugin: ToposPlugin): MarkdownPostProcessor {
  return (el) => {
    if (plugin.settings.linkInReading) linkElement(plugin, el);
  };
}

/** Highlights the references in an element's text, leaving links and highlighted ones alone */
export function linkElement(plugin: ToposPlugin, el: HTMLElement): void {
  const doc = el.ownerDocument;
  const walker = doc.createTreeWalker(el, NodeFilter.SHOW_TEXT, {
    acceptNode(node) {
      for (let parent = node.parentElement; parent && parent !== el; parent = parent.parentElement) {
        // A table cell being edited is an editor of its own, with the editor's highlights
        if (SKIP.has(parent.tagName) || parent.classList.contains("cm-editor") || parent.classList.contains(RENDERED)) {
          return NodeFilter.FILTER_REJECT;
        }
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
    const fragment = doc.createDocumentFragment();
    let last = 0;
    for (const m of matches) {
      const url = plugin.referenceUrl(m.passage);
      if (!url) continue;
      fragment.append(text.slice(last, m.start));
      const reference = doc.createElement("span");
      reference.className = `topos-reference ${RENDERED}`;
      reference.textContent = text.slice(m.start, m.end);
      reference.dataset.toposUrl = url;
      reference.title = `${m.passage.reference}, opens in ${plugin.linkSite()}`;
      const open = (event: MouseEvent) => {
        if (!plugin.clickOpens(event, false)) return;
        // In live preview, a click in a callout would otherwise start editing it
        event.preventDefault();
        event.stopPropagation();
        doc.defaultView?.open(reference.dataset.toposUrl, "_blank");
      };
      reference.addEventListener("click", open);
      // A middle click arrives as auxclick; its mousedown would paste (Linux) or autoscroll
      reference.addEventListener("auxclick", open);
      reference.addEventListener("mousedown", (event) => {
        if (event.button === 1) event.preventDefault();
      });
      fragment.append(reference);
      last = m.end;
    }
    fragment.append(text.slice(last));
    node.replaceWith(fragment);
  }
}

/** Turns an element's highlighted references back into text */
export function unlinkElement(el: HTMLElement): void {
  const links = el.querySelectorAll(`.${RENDERED}`);
  for (const link of Array.from(links)) link.replaceWith(link.textContent ?? "");
  if (links.length > 0) el.normalize();
}

/** Relinks the rendered blocks in a live preview editor for the current settings */
export function relinkEditor(plugin: ToposPlugin, editorEl: HTMLElement): void {
  for (const block of Array.from(editorEl.querySelectorAll<HTMLElement>(RENDERED_IN_EDITOR))) {
    // Embedded notes are inside embed blocks too; relink each block only once
    if (block.parentElement?.closest(RENDERED_IN_EDITOR)) continue;
    unlinkElement(block);
    if (plugin.settings.linkInReading) linkElement(plugin, block);
  }
}
