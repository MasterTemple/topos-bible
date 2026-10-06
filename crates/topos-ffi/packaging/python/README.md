# topos-bible

Find, parse, and complete Bible references in text. Fast (written in Rust), typed, and
without dependencies.

```sh
pip install topos-bible
```

Wheels are published for Linux (x86_64 and aarch64), macOS (Apple Silicon and Intel), and
Windows, on Python 3.10 and later.

## Usage

```python
from topos_bible import BookStyle, OffsetUnit, Topos

topos = Topos()

for m in topos.search("Read Jn 3:16-18 and Rom 8:28", OffsetUnit.CHAR):
    print(m.passage.reference, m.passage.osis, m.start, m.end)
# John 3:16-18 John.3.16-John.3.18 5 15
# Romans 8:28 Rom.8.28 20 28

topos.parse("1 Cor 13:4-7", BookStyle.NAME).reference
# '1 Corinthians 13:4-7'

# Completions replace text[start:end] with completion.text
[c.text for c in topos.complete("Gen 1:", 6, OffsetUnit.CHAR, BookStyle.NAME, 2)]
# ['Genesis 1:1', 'Genesis 1:2']
```

The package is imported as `topos_bible`.

## API

| | |
|---|---|
| `Topos()` | Default English book names and versification |
| `Topos.with_config(json)` | Custom `books`, `genres`, or `chapter_verses`; raises `ToposErrorException` if the JSON is invalid |
| `search(text, unit)` | Every reference in `text`, as `Match(passage, start, end, line, column)` |
| `parse(reference, style)` | One reference (`Jn 3:16` or OSIS `John.3.16`) as a `Passage`, or `None` |
| `complete(text, cursor, unit, style, limit)` | Completions for the reference ending at `cursor` (`limit` 0 means no limit) |
| `verse_ranges(passage)` | Each segment as a `VerseRange(start, end)`, with whole chapters expanded to their verses |
| `verses(passage)` | Every verse in the passage, in order, as `ChapterVerse(chapter, verse)` |
| `contains(outer, inner)` / `overlaps(a, b)` | Whether one passage contains, or shares any verse with, another |
| `search_with(text, unit, query)` | The references a `ToposQuery` keeps |
| `contradiction(query)` | Why nothing can match, if the query's filters contradict each other |

- `Passage(book_id, book, reference, segments, osis)`, with `reference` written in the
  requested `BookStyle` (`NAME`, `ABBREVIATION`, or `OSIS`)
- `Completion(label, kind, start, end, text)`, where `kind` is a `CompletionKind`
  (`BOOK`, `CHAPTER`, or `VERSE`)
- **Offsets:** pass `OffsetUnit.CHAR` so offsets match Python string indices (`BYTE` and
  `UTF16` are also available)

### Segments

`passage.segments` lists each part of the reference as written. Each is either a
`PassageSegmentVerses(start, end)` with `ChapterVerse` ends, or a
`PassageSegmentChapters(start, end)` with chapter numbers; `end` is `None` for a single verse
or chapter:

```python
from topos_bible import PassageSegmentChapters, PassageSegmentVerses

passage = topos.parse("John 3:16-18,20-4:2; 5", BookStyle.NAME)
for segment in passage.segments:
    match segment:
        case PassageSegmentVerses(start=start, end=end):
            print("verses", start.chapter, start.verse, end)
        case PassageSegmentChapters(start=start, end=end):
            print("chapters", start, end)

topos.verse_ranges(passage)  # whole chapters with real verse numbers: John 5 is 5:1 to 5:47
len(topos.verses(passage))   # every verse, one by one
```

### Queries and options

Both are builders: each method returns a new value, like the CLI's options.

```python
from topos_bible import OffsetUnit, Topos, ToposOptions, ToposQuery

query = (
    ToposQuery.create()
    .new_testament()                  # --nt; also old_testament(), testament(), exclude_testament()
    .genre("Pauline Epistles")        # -g; also book(), exclude_genre(), exclude_book()
    .explicit_overlap("Romans 8")     # -o: names a verse of Romans 8 (whole chapters don't count)
    .exclude_overlap("Romans 8:28")   # also inside(), any_overlap(), exact_overlap()
)
matches = Topos().search_with(text, OffsetUnit.CHAR, query)

topos = (
    ToposOptions.create()
    .merge_data('{"books":[{"book":"John","abbreviations":["jhn"]}]}')  # also data(), remove_data()
    .context_book("John")                                              # also context_heading()
    .build()
)
```

How references are written is a third builder, `ToposFormat` (the CLI's `--psg-fmt`):

```python
from topos_bible import BookStyle, ToposFormat

fmt = ToposFormat.create().book(BookStyle.ABBREVIATION).join_adjacent(True)  # also book_separator(),
# chapter_verse(), range(), verse_separator(), chapter_separator(), omit_first_verse_of_chapter_range(),
# chapter_in_single_chapter_books(), and with_json('{"join_adjacent": true}')
short = ToposOptions.create().format(fmt).build()  # search results: "Jn 3:16-18"
short.complete_with(text, cursor, OffsetUnit.CHAR, fmt, 20)
short.format_passage(passage, fmt)
```

### Errors

```python
from topos_bible import ToposErrorException

try:
    custom = ToposOptions.create().data(open("bible.json").read()).build()
except ToposErrorException as error:
    print(error.error.message)
```

Invalid queries (an unknown book, genre, or passage) raise it too.

`search` finds abbreviations (`Jn`, `1 Co`), ranges and lists (`5:1-3,5; 6:6`), `ff`, Roman
numerals (`Matth. x, 8`), and words split across lines, while skipping false positives like
`is 2.5%`.

Source, the CLI, and other languages: [github.com/MasterTemple/topos-bible](https://github.com/MasterTemple/topos-bible).
Released under [CC0 1.0](https://creativecommons.org/publicdomain/zero/1.0/).
