# topos-bible

Find, parse, and complete Bible references in text, from Python, JavaScript, Swift, and
Kotlin. Built on the Rust [topos](https://github.com/MasterTemple/topos-bible) library.

```python
from topos_bible import BookStyle, OffsetUnit, Topos

topos = Topos()
for m in topos.search("Read Jn 3:16-18 and Rom 8:28", OffsetUnit.CHAR):
    print(m.passage.reference, m.passage.osis, m.start, m.end)
# John 3:16-18 John.3.16-John.3.18 5 15
# Romans 8:28 Rom.8.28 20 28

topos.parse("1 Cor 13:4-7", BookStyle.NAME).reference      # '1 Corinthians 13:4-7'
[c.text for c in topos.complete("Gen 1:", 6, OffsetUnit.CHAR, BookStyle.NAME, 2)]
# ['Genesis 1:1', 'Genesis 1:2']
```

```js
import { initialized, Topos, OffsetUnit } from "topos-bible";
await initialized;
const topos = Topos.new();
topos.search("Read Jn 3:16-18", OffsetUnit.Utf16); // offsets are UTF-16, like JS strings
```

- `search(text, unit)` finds every reference: abbreviations (`Jn`, `1 Co`), ranges and lists
  (`5:1-3,5; 6:6`), `ff`, Roman numerals (`Matth. x, 8`), and hyphenated line breaks, while
  avoiding false positives like `is 2.5%`
- `parse(reference, style)` reads one reference, or an OSIS id like `John.3.16`
- `complete(text, cursor, unit, style, limit)` completes the reference being typed, as edits
- `Topos.with_config(json)` uses custom book names, genres, or versification

Offsets use the unit you pass: `Char` for Python, `Utf16` for JavaScript. See the
[documentation](https://github.com/MasterTemple/topos-bible/tree/main/crates/topos-ffi) for details.

Released under [CC0 1.0](https://creativecommons.org/publicdomain/zero/1.0/).
