# topos-bible-index

A compact, persistent index of the Bible references in many files, for apps that search the same
files again and again: a notes vault, a library of books. The Obsidian plugin keeps over a million
references in it, on desktop and on phones.

- **Compact**: each file's references take about 15 bytes each (a hand-written codec), and load
  at about 25 million a second
- **Only changed files are searched**: `Index::check` compares a file's size and modification
  time with its entry
- **Shared between devices**: entries can come from another device that syncs the same files.
  Modification times change when files sync, so a text file is then matched by a hash of its
  contents (`Index::confirm`), or a big file (an EPUB) by its size. Each device writes only its
  own `Pack`s, so syncing never conflicts
- **EPUBs** keep their CFIs and the text around each reference in a separate `Detail` per
  book, read only when a reference is shown or opened
- **Queries** filter (with any test of a passage, like the CLI's filters), sort (by file or by
  Bible order), count, and page in Rust; each order is kept until the index changes

The `topos` CLI writes entries with `-m index`, and the bindings (`topos-ffi`) expose the index as
`ToposIndex`.
