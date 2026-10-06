// Run with `npm install && npm start` after `boltffi pack wasm` (see ../../README.md)
// In Node and Bun the WebAssembly module is ready on import (browsers: `await initialized` first)
import { Topos, OffsetUnit, BookStyle, CompletionKind } from "topos-bible";

const topos = Topos.new();

// JavaScript strings are UTF-16, so ask for UTF-16 offsets
const text = "Notes 📖: read Jn 3:16-18 and Rom 8:28.";
for (const m of topos.search(text, OffsetUnit.Utf16)) {
  console.log(`${m.passage.reference} (${m.passage.osis}) at ${m.start}..${m.end}:`, text.slice(m.start, m.end));
}

console.log(topos.parse("1 Cor 13:4-7", BookStyle.Name)?.reference);

// Completions replace text[start..end] with completion.text
const input = "see Gen 1:";
for (const c of topos.complete(input, input.length, OffsetUnit.Utf16, BookStyle.Name, 3)) {
  const kind = Object.keys(CompletionKind).find((k) => CompletionKind[k as keyof typeof CompletionKind] === c.kind);
  console.log(kind, input.slice(0, c.start) + c.text + input.slice(c.end));
}

topos.dispose();
