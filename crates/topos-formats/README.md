# topos-bible-formats

Find Bible references in documents and report where each one is in that document's own terms,
using [`topos-bible`](https://crates.io/crates/topos-bible):

| Format | Feature | Location |
|---|---|---|
| HTML | `html` | Line and column, and a text fragment (`#:~:text=`) to link straight to it |
| SRT, WebVTT, SBV | `srt` | The cue and its start and end times |
| EPUB | `epub` | A range CFI, the same one the [EPUB++](https://github.com/MasterTemple/epub-plus-plus) reader makes (`epub::epub_link` writes its links) |
| JSON | `json` | A JSON Pointer |
| XML | `xml` | An element path |
| PDF | `pdf` | The page and the text's rectangles (needs MuPDF, so it is off by default) |

```rust
use topos_bible::matcher::BibleMatcher;
use topos_bible_formats::{SearchFormat, srt::SRTLocation};

let srt = "1\n00:00:01,000 --> 00:00:04,000\nTurn to John 3:16.\n\n2\n00:00:05,500 --> 00:00:08,000\nAnd Romans 8:28.\n";
let found = BibleMatcher::default().search_format::<SRTLocation>(srt)?;

assert_eq!(found.len(), 2);
assert_eq!(found[1].location.id, 2);
assert_eq!((found[1].location.start.seconds, found[1].location.start.millis), (5, 500));
# Ok::<(), topos_bible_formats::FormatError>(())
```

Filters, custom data, and book context work as in `topos-bible`: build the `BibleMatcher` with
them, then search any format.

The `htmloc` module (formerly its own crate) converts between HTML positions and text fragments.

## License

CC0-1.0
