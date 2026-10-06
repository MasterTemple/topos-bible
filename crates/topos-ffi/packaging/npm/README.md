# topos-bible

Find, parse, and complete Bible references in text. Runs anywhere JavaScript does, via
WebAssembly, and ships with TypeScript types.

```sh
npm install topos-bible   # or: bun add topos-bible
```

## Usage

```ts
import { Topos, OffsetUnit, BookStyle } from "topos-bible";

const topos = Topos.new();

for (const m of topos.search("Read Jn 3:16-18 and Rom 8:28", OffsetUnit.Utf16)) {
  console.log(m.passage.reference, m.passage.osis, m.start, m.end);
}
// John 3:16-18 John.3.16-John.3.18 5 15
// Romans 8:28 Rom.8.28 20 28

topos.parse("1 Cor 13:4-7", BookStyle.Name)?.reference; // "1 Corinthians 13:4-7"

// Completions replace text.slice(start, end) with completion.text
const input = "see Gen 1:";
const [first] = topos.complete(input, input.length, OffsetUnit.Utf16, BookStyle.Name, 10);
input.slice(0, first.start) + first.text + input.slice(first.end); // "see Genesis 1:1"
```

### Browsers and bundlers

In Node and Bun the WebAssembly module loads as the package is imported, so it is ready
right away. In browsers and with bundlers it is fetched asynchronously; wait for it before the
first call:

```ts
import { initialized, Topos } from "topos-bible";

await initialized;
const topos = Topos.new();
```

## API

| | |
|---|---|
| `Topos.new()` | Default English book names and versification |
| `Topos.withConfig(json)` | Custom `books`, `genres`, or `chapter_verses` (returns `null` if the JSON is invalid) |
| `search(text, unit)` | Every reference in `text`, as `Match { passage, start, end, line, column }` |
| `parse(reference, style)` | One reference (`Jn 3:16` or OSIS `John.3.16`) as a `Passage`, or `null` |
| `complete(text, cursor, unit, style, limit)` | Completions for the reference ending at `cursor` (`limit` 0 means no limit) |
| `verseRanges(passage)` | Each segment as a `VerseRange { start, end }`, with whole chapters expanded to their verses |
| `verses(passage)` | Every verse in the passage, in order, as `ChapterVerse { chapter, verse }` |
| `contains(outer, inner)` / `overlaps(a, b)` | Whether one passage contains, or shares any verse with, another |
| `searchWith(text, unit, query)` | The references a `ToposQuery` keeps |
| `contradiction(query)` | Why nothing can match, if the query's filters contradict each other |
| `dispose()` | Frees the Rust object now instead of waiting for garbage collection |

- `Passage { bookId, book, reference, segments, osis }`, with `reference` written in the
  requested `BookStyle` (`Name`, `Abbreviation`, or `Osis`)
- `Completion { label, kind, start, end, text }`, where `kind` is a `CompletionKind`
  (`Book`, `Chapter`, or `Verse`)
- **Offsets:** pass `OffsetUnit.Utf16` so offsets match JavaScript string indices (`Byte` and
  `Char` are also available)

### Queries and options

Both are builders: each method returns a new value, like the CLI's options.

```ts
import { Topos, ToposQuery, ToposOptions, ToposErrorException, OffsetUnit } from "topos-bible";

const query = ToposQuery.create()
  .newTestament()                 // --nt; also oldTestament(), testament(), excludeTestament()
  .genre("Pauline Epistles")      // -g; also book(), excludeGenre(), excludeBook()
  .explicitOverlap("Romans 8")    // -o: names a verse of Romans 8 (whole chapters don't count)
  .excludeOverlap("Romans 8:28"); // also inside(), anyOverlap(), exactOverlap()
const matches = Topos.new().searchWith(text, OffsetUnit.Utf16, query);

const topos = ToposOptions.create()
  .mergeData('{"books":[{"book":"John","abbreviations":["jhn"]}]}') // also data(), removeData()
  .contextBook("John")                                             // also contextHeading()
  .build();
```

Invalid queries and options throw `ToposErrorException`, with the reason in
`error.value.message` (an unknown book, a name that would mean two books, ...). Prefer
`ToposOptions.create().data(json).build()` to `Topos.withConfig(json)`, which returns `null`
instead of throwing.

### Segments

`passage.segments` lists each part of the reference as written. Each is either verses or whole
chapters, and `end` is `null` for a single verse or chapter:

```ts
// John 3:16-18,20-4:2; 5
[
  { tag: "Verses", start: { chapter: 3, verse: 16 }, end: { chapter: 3, verse: 18 } },
  { tag: "Verses", start: { chapter: 3, verse: 20 }, end: { chapter: 4, verse: 2 } },
  { tag: "Chapters", start: 5, end: null },
]

for (const segment of passage.segments) {
  if (segment.tag === "Verses") {
    const last = segment.end ?? segment.start; // ChapterVerse
  } else {
    const lastChapter = segment.end ?? segment.start; // number
  }
}

topos.verseRanges(passage); // whole chapters with real verse numbers: John 3 is 3:1 to 3:36
topos.verses(passage).length; // every verse, one by one
```

`search` finds abbreviations (`Jn`, `1 Co`), ranges and lists (`5:1-3,5; 6:6`), `ff`, Roman
numerals (`Matth. x, 8`), and words split across lines, while skipping false positives like
`is 2.5%`.

Source, the CLI, and other languages: [github.com/MasterTemple/topos-bible](https://github.com/MasterTemple/topos-bible).
Released under [CC0 1.0](https://creativecommons.org/publicdomain/zero/1.0/).
