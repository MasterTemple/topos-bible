# topos-bible for Swift

Find, parse, and complete Bible references in text, from Swift on iOS and macOS.

## Build

On macOS with Xcode, from `crates/topos-ffi` (with `cargo install boltffi_cli --version 0.31.0`):

```sh
boltffi pack apple
```

This writes an XCFramework and a `Package.swift` to `dist/apple`. Add that folder to your
project as a local Swift package (**File → Add Package Dependencies → Add Local**). The
module is named `Topos` (`module_name` in `boltffi.toml`).

## Usage

```swift
import Topos

let topos = Topos()

for m in topos.search(text: "Read Jn 3:16-18", unit: .utf16) {
    print(m.passage.reference, m.passage.osis, m.start, m.end)
}

let passage = topos.parse(reference: "1 Cor 13:4-7", style: .name)  // Passage?

// Completions replace the UTF-16 range start..<end with completion.text
let input = "see Gen 1:"
let completions = topos.complete(text: input, cursor: UInt32(input.utf16.count),
                                 unit: .utf16, style: .name, limit: 10)

for segment in passage?.segments ?? [] {
    switch segment {
    case let .verses(start, end):     // end is nil for a single verse
        print(start.chapter, start.verse, end as Any)
    case let .chapters(start, end):   // end is nil for a single chapter
        print(start, end as Any)
    }
}
let ranges = topos.verseRanges(passage: passage!)   // whole chapters expanded to verses
let verses = topos.verses(passage: passage!)        // every verse, one by one

do {
    let custom = try Topos(withConfig: json)
} catch ToposError.invalidConfig(let message) {
    print(message)
}
```

- `Passage { bookId, book, reference, segments, osis }`, `Match { passage, start, end, line,
  column }`, and `Completion { label, kind, start, end, text }` are value types
- `segments` is `[PassageSegment]`: `.verses(start:end:)` with `ChapterVerse` ends, or
  `.chapters(start:end:)`
- Helpers: `verseRanges(passage:)`, `verses(passage:)`, `contains(outer:inner:)`, and
  `overlaps(a:b:)`
- **Offsets:** pass `.utf16` and use `String.utf16` indices (`.byte` and `.char` also exist)

This snippet follows the generated bindings but has not been compiled in CI. See the
[main documentation](../../README.md) for the full API.
