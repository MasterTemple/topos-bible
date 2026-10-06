# topos-bible for Kotlin

Find, parse, and complete Bible references in text, from Kotlin on Android.

## Build

With the Android NDK installed, from `crates/topos-ffi` (with
`cargo install boltffi_cli --version 0.31.0`):

```sh
boltffi pack android
```

This writes native libraries to `dist/android/jniLibs` and Kotlin sources to
`dist/android/kotlin`. Add both to an Android module (`jniLibs` under `src/main`, and the
Kotlin sources as a source set). The package is `io.github.mastertemple.topos` (set in
`boltffi.toml`).

## Usage

```kotlin
import io.github.mastertemple.topos.*

Topos().use { topos ->  // AutoCloseable: frees the Rust object at the end
    for (m in topos.search("Read Jn 3:16-18", OffsetUnit.UTF16)) {
        println("${m.passage.reference} ${m.passage.osis} ${m.start}..${m.end}")
    }

    val passage = topos.parse("1 Cor 13:4-7", BookStyle.NAME)  // Passage?

    for (segment in passage?.segments.orEmpty()) {
        when (segment) {
            is PassageSegment.Verses -> println("${segment.start} ${segment.end}")  // end null: one verse
            is PassageSegment.Chapters -> println("${segment.start} ${segment.end}")  // end null: one chapter
        }
    }
    val verses = passage?.let { topos.verses(it) }  // every verse, one by one

    // Completions replace input.substring(start, end) with completion.text
    val input = "see Gen 1:"
    val completions = topos.complete(input, input.length.toUInt(), OffsetUnit.UTF16, BookStyle.NAME, 10u)
}

try {
    val custom = Topos(json)
} catch (error: ToposError.InvalidConfig) {
    println(error.message)
}
```

- `passage.segments` is a `List<PassageSegment>`: `Verses(start, end)` with `ChapterVerse`
  ends, or `Chapters(start, end)`
- Helpers: `verseRanges(passage)`, `verses(passage)`, `contains(outer, inner)`, and
  `overlaps(a, b)`
- Offsets and counts are `UInt` (chapters and verses are `UByte`)
- **Offsets:** pass `OffsetUnit.UTF16`, which matches Kotlin and Java string indices
  (`BYTE` and `CHAR` also exist)

This snippet follows the generated bindings but has not been compiled in CI. See the
[main documentation](../../README.md) for the full API.
