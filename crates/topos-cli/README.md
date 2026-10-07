# τόπος Bible CLI

## Examples

*Results are truncated. These examples use `-m table`; on a terminal the default output groups results by file, and when piped it prints `path:line:column: reference`.*

## Install

```sh
cargo install topos-bible-cli
```

This installs the `topos` command. Add `--features pdf` to also search PDFs (it builds MuPDF,
which takes a few minutes). From a clone, use `cargo install --path crates/topos-cli`.

### Default Search

Search for all Bible verses in current directory recursively (respecting `.gitignore`)

**Command**

```bash
topos -m table
```

**Output**

```
| File                 | Line | Col | Verse              |
| ----                 | ---- | --- | -----              |
| ./Church 07-27-25.md | 24   | 12  | Colossians 2:1-3   |
| ./Church 07-27-25.md | 39   | 12  | Ephesians 6:18     |
| ./Church 07-20-25.md | 6    | 6   | Colossians 3:12-15 |
| ./Church 07-20-25.md | 27   | 12  | Colossians 3:12    |
| ./Church 07-20-25.md | 34   | 12  | Deuteronomy 7:6-8  |
```

### Search Specific File

**Command**

```bash
topos "Church 07-27-25.md" -m table
```

**Output**

```
| File               | Line | Col | Verse            |
| ----               | ---- | --- | -----            |
| Church 07-27-25.md | 24   | 12  | Colossians 2:1-3 |
| Church 07-27-25.md | 39   | 12  | Ephesians 6:18   |
| Church 07-27-25.md | 46   | 12  | Colossians 4:12  |
| Church 07-27-25.md | 49   | 12  | Colossians 2:1   |
| Church 07-27-25.md | 56   | 12  | Romans 16:1-2    |
| Church 07-27-25.md | 61   | 12  | 3 John 5-8       |
..
```

### Filter by Testament

**Command**

```bash
topos --nt -m table   # same as -t new
```

**Output**

```
| File                 | Line | Col | Verse            |
| ----                 | ---- | --- | -----            |
| ./Church 07-27-25.md | 24   | 12  | Colossians 2:1-3 |
| ./Church 07-27-25.md | 39   | 12  | Ephesians 6:18   |
| ./Church 07-27-25.md | 46   | 12  | Colossians 4:12  |
| ./Church 07-27-25.md | 49   | 12  | Colossians 2:1   |
```

### Filter by Genre

**Command**

```bash
topos -g wisdom -m table
```

**Output**

```
| File                 | Line | Col | Verse             |
| ----                 | ---- | --- | -----             |
| ./Church 07-20-25.md | 143  | 12  | Proverbs 16:32    |
| ./Church 07-13-25.md | 229  | 12  | Proverbs 4:24     |
| ./Church 03-02-25.md | 281  | 12  | Proverbs 20:27    |
| ./Church 03-02-25.md | 329  | 12  | Proverbs 28:9     |
| ./Church 06-15-25.md | 68   | 12  | Ecclesiastes 9:10 |
```

### Filter by Book

**Command**

```bash
topos -b Romans -m table
```

**Output**

```
| File                 | Line | Col | Verse         |
| ----                 | ---- | --- | -----         |
| ./Church 07-27-25.md | 56   | 12  | Romans 16:1-2 |
| ./Church 07-20-25.md | 215  | 17  | Romans 15     |
| ./Church 07-20-25.md | 217  | 12  | Romans 15:11  |
| ./Church 07-20-25.md | 271  | 12  | Romans 12:1-2 |
| ./Church 07-13-25.md | 268  | 12  | Romans 12:9   |
| ./Church 07-06-25.md | 98   | 12  | Romans 10:9   |
| ./Church 06-29-25.md | 80   | 12  | Romans 11:29  |
| ./Church 06-22-25.md | 198  | 12  | Romans 12:2   |
| ./Church 02-23-25.md | 224  | 8   | Romans 8      |
```

### Filter by Passage

**Command**

```bash
topos --any-overlap "1 Peter 1:1-5, 4:11,14-16" -m table
```

**Output**

```
| File                 | Line | Col | Verse         |
| ----                 | ---- | --- | -----         |
| ./Church 03-02-25.md | 118  | 12  | 1 Peter 1:2   |
| ./Church 06-01-25.md | 24   | 101 | 1 Peter 4     |
| ./Church 05-11-25.md | 334  | 12  | 1 Peter 4:15  |
| ./Church 02-02-25.md | 39   | 12  | 1 Peter 4:11  |
| ./Church 03-09-25.md | 241  | 12  | 1 Peter 1:3-4 |
```

| Option | Keeps references that | `John 3:16` keeps |
|---|---|---|
| `--any-overlap` | share any verse with the passage | `John 3`, `John 3:14-18`, `John 2; 3:16` |
| `-o`, `--explicit-overlap` | name a verse of the passage (whole chapters don't count) | `John 3:14-18`, `John 2; 3:16` |
| `-i`, `--inside` | are entirely inside the passage | `John 3:16` |
| `--exact-overlap` | are exactly the passage, however written | `Jn 3:16`, not `John 3:16-17` |
| `--exclude-overlap` | share no verse with the passage | everything else |

So `1 Peter 4` above is kept by `--any-overlap`, but not by `-o` (it names no verse) or `-i`
(it isn't inside the passage). The options that keep references can be combined and repeated
(a reference passes if any one keeps it); `--exclude-overlap` then removes references.

### Exclude Testament/Genre/Book/Passage

Use just like above, but prefix the full option with `exclude` (`--exclude-overlap` for passages)

```bash
topos --exclude-testament new -m table
```

```
| File                 | Line | Col | Verse             |
| ----                 | ---- | --- | -----             |
| ./Church 07-20-25.md | 34   | 12  | Deuteronomy 7:6-8 |
| ./Church 07-20-25.md | 108  | 93  | Numbers 12        |
| ./Church 07-20-25.md | 131  | 12  | Isaiah 53:7       |
| ./Church 07-20-25.md | 143  | 12  | Proverbs 16:32    |
| ./Church 07-20-25.md | 204  | 7   | Psalm 117         |
| ./Church 07-20-25.md | 208  | 12  | Psalm 117:1-2     |
| ./Church 07-20-25.md | 234  | 12  | Habakkuk 2:14     |
| ./Church 07-20-25.md | 255  | 12  | Psalm 117:2       |
```

## Rules

- Including a testament limits the search to it (`--nt -g Gospels` is the four Gospels)
- Included genres and books add up (`-g Pentateuch -b Revelation` is six books)
- Filters that can't match anything print a warning: genres or books outside the included
  testaments (`--ot -g "Pauline Epistles"`), passages to keep only in books that aren't searched
  (`-b Genesis -i "Romans 8"`), or only within excluded passages (`-i "John 3:16"
  --exclude-overlap "John 3"`)
- Exclusions always win, so a book can be excluded from an included genre
- `-m count` prints matches per file; `--total-count` prints one total across all files
- Passage filters that keep references (`-i`, `--any-overlap`, `-o`, `--exact-overlap`) are joined
  with a logical OR, then `--exclude-overlap` removes references
- `--overlaps` and `--outside` still work, as old names for `--any-overlap` and `--exclude-overlap`
- Unknown books or genres are errors

## Config

Default options go in `~/.config/topos/config.toml` (or `$XDG_CONFIG_HOME/topos/config.toml`).
The first run writes a commented `config.toml` and a `queries.toml` with a sample query there if
they don't exist (it never overwrites them, and doesn't touch the folder with `--no-config`).
Each key is a long option name; the command line overrides them. Use `--config PATH` to read another file instead, or `--no-config` to read none.

```toml
mode = "grouped"
color = "always"
cache = true
exclude-book = ["Song of Solomon"]
data = "~/bible/custom.json"   # custom books, genres, or chapter and verse counts
merge-data = ["~/bible/more-names.json"]
remove-data = "~/bible/fewer-names.json"
```

## Custom data

`--data FILE` replaces the built-in books, genres, or chapter and verse counts (whichever parts
the file has). To change only a few things, keep the defaults and use either of these, or both.
They can be repeated and set in `config.toml`:

- `--merge-data FILE` adds names and values. A book is found by its `id`, or else by any of its
  names. A book that isn't found is added, so it needs an `id`. Renaming a book keeps its old
  name as an abbreviation, and chapter counts replace the book's.
- `--remove-data FILE` removes values. A book or genre listed with nothing else is removed
  entirely, otherwise just the values listed are removed.

The files use the `--data` format with every field optional. Values already there aren't added
twice, and every name must still mean exactly one book (`"gen"` for John is an error).

```jsonc
// more-names.json
{
  "books": [
    { "book": "John", "abbreviations": ["jhn"] },
    { "id": 67, "book": "Tobit", "abbreviation": "Tob", "abbreviations": ["tb"] }
  ],
  "genres": [{ "title": "Apocrypha", "books": ["Tobit"] }],
  "chapter_verses": { "Tobit": [22, 14, 17, 21, 22, 18, 16, 21, 6, 13, 18, 22, 18, 15] }
}
// fewer-names.json: "song" no longer matches; Jude isn't a book
{ "books": [{ "book": "Song of Solomon", "abbreviations": ["song"] }, { "book": "Jude" }] }
```

The order is `--data` (or the defaults), then each `--merge-data`, then each `--remove-data`.

## Named queries

Name searches you repeat in `~/.config/topos/queries.toml` (next to `config.toml`), and use them
with `-q NAME`:

```toml
paul = '--nt -g "Pauline Epistles"'
sermons = 'Sermons -o "John 1" --exclude-book Philemon'
gospels = ["-g", "Gospels"]   # or the arguments already split
```

```sh
topos -q paul ~/notes          # topos --nt -g "Pauline Epistles" ~/notes
topos -q sermons -m count      # options after -q add to (or override) the query's
topos --list-queries
```

`-q` is replaced by the query's options where it appears, so queries can use other queries, and
`config.toml` can set a default one (`query = "paul"`). The file is read even with `--no-config`.
The Obsidian plugin's saved searches use the same syntax.

## Cache

The default `config.toml` turns the cache on (`cache = true`); `--no-cache` skips it for one
run, and `--cache` turns it on without a config. It keeps each file's references between runs,
until the file changes (its size or modification time). Results are stored **before filtering**, so
one run serves every later search of the same files, whatever its filters. On a vault with 2,000
EPUBs and notes:

| | Time |
|---|---|
| First run | 5.7 s |
| Again, with the same or any other filters (`--nt`, `-b John`, `-o "Romans 8"`) | 0.1–0.16 s |

- Only `--data`, `--context-book`, `--context-heading`, and the version get a separate cache
- Each file has a small binary entry in `~/.cache/topos` (or `$XDG_CACHE_HOME/topos`) that
  records which books it mentions, so files that can't match the filters are skipped
- Entries for deleted files are removed now and then, as are caches unused for a month;
  `topos --clear-cache` deletes everything

## Writing references

`-f` picks the book style (`name`, `abbreviation`, `osis`). The rest of how references are written,
in results and completions, comes from `--psg-fmt` (the whole format as JSON) or one `--fmt-*`
option per field, which override it:

| Field | Option | Default |
|---|---|---|
| `book` | `-f` | `name` |
| `book_separator` | `--fmt-book-separator` | `" "` |
| `chapter_verse` | `--fmt-chapter-verse` | `":"` |
| `range` | `--fmt-range` | `"-"` |
| `verse_separator` | `--fmt-verse-separator` | `","` |
| `chapter_separator` | `--fmt-chapter-separator` | `"; "` |
| `join_adjacent` (`3:16-18`, not `3:16,17,18`) | `--fmt-join-adjacent` | `false` |
| `omit_first_verse_of_chapter_range` (`1-2:3`, not `1:1-2:3`) | `--fmt-omit-first-verse-of-chapter-range` | `false` |
| `chapter_in_single_chapter_books` (`Jude 1:5`, not `Jude 5`) | `--fmt-chapter-in-single-chapter-books` | `true` |

```sh
topos --psg-fmt '{"join_adjacent": true, "range": "–"}' notes/
topos --fmt-join-adjacent --fmt-chapter-in-single-chapter-books=false notes/
```

In `config.toml`, `--psg-fmt` is a table, and the yes/no options take `true` or `false`:

```toml
psg-fmt = { join_adjacent = true, verse_separator = ", " }
fmt-chapter-in-single-chapter-books = false
```

## Completing references

`--complete TEXT` prints the completions for a partly typed reference, one per line, for scripts
and editors; `--list-books` (the same as `--complete ""`) prints every book:

```sh
topos --complete "John 3:"        # John 3:1 ... John 3:36
topos --complete "jn 3:16-"       # John 3:16-17, John 3:16-18, ...
topos --list-books -f abbreviation --nt
topos --complete "jn 3:" -f osis  # John.3.1 ...
topos --complete "Rom 8" -m json  # {"text":"Romans 8","label":"Romans 8","kind":"chapter"}
```

`-f`, `--psg-fmt`, and `--fmt-*` set how completions are written, the book and passage filters
narrow the list, and `-m json` prints
each completion's text, label, and kind (book, chapter, or verse).

## Shell completions

Tab completes options and their values: modes and formats, book and genre names (`-b jn` offers
Jonah and John; `-b "1 Co` gives `"1 Corinthians"`), testaments, your named queries, references
for the passage filters (books, then chapters, verses, and range ends, like the editor plugins),
and paths. `topos` computes them itself when Tab is pressed, so they always match the installed
version and your `queries.toml`.

```sh
# bash: load on demand
COMPLETE=bash topos > ~/.local/share/bash-completion/completions/topos
# or in ~/.bashrc
source <(COMPLETE=bash topos)

# zsh (~/.zshrc), fish (~/.config/fish/config.fish), elvish, powershell
source <(COMPLETE=zsh topos)
COMPLETE=fish topos | source
```

Quote references (`-o "John 3:16"`): bash splits words at `:`.

## Choosing files

Like ripgrep, `topos` skips files ignored by `.gitignore` (inside Git repositories), `.ignore`, and
`.toposignore`, as well as hidden files and binary files. Files named on the command line are
always searched.

| Option | Effect |
|---|---|
| `--hidden`, `-.` | Search hidden files and directories |
| `--no-ignore` | Don't respect any ignore files |
| `--no-ignore-vcs` | Don't respect Git's ignore files (but keep `.ignore` and `.toposignore`) |
| `--no-ignore-parent` | Don't respect ignore files in parent directories |
| `--no-require-git` | Respect `.gitignore` outside Git repositories too |
| `--ignore-file PATH` | Also ignore the paths in this file |
| `-u`, `-uu`, `-uuu` | `--no-ignore`, then also `--hidden`, then also `--binary` |
| `--binary` | Search files that look binary as text |
| `--glob GLOB`, `--iglob GLOB` | Include (or with `!`, exclude) matching paths; globs override ignore files and hidden |
| `--ext md,txt`, `--exclude-ext pdf` | Only, or never, these extensions |
| `-d NUM`, `--max-depth NUM` | Descend at most this many directories |
| `--max-filesize SIZE` | Skip larger files (`500K`, `10M`, `1G`) |
| `-L`, `--follow` | Follow symbolic links |
| `--one-file-system` | Stay on the file system of each path given |

To see which files a search would cover, `--files` lists them without searching. `-l` lists the
files with references, and `--files-without-match` lists the ones without.

## File types

- `.pdf` (with the `pdf` feature) reports the page, and `.epub` reports a range CFI like
  `epubcfi(/6/14!/4/2/4,/1:0,/1:16)`, the same one the [EPUB++](https://github.com/MasterTemple/epub-plus-plus)
  reader makes for that text (`--cfi-assertions` adds `[id]` assertions)
- `.srt`, `.vtt`, and `.sbv` also report the cue's start time
- Everything else is searched as text; binary files are skipped (see `--binary`)

## EPUB++ links

`--epub-links wiki` (or `markdown`) searches only EPUBs and prints a link to each reference that
the EPUB++ Obsidian plugin opens at the reference, labeled with it (as `-f` and `--psg-fmt` write
it). Paths are relative to the directory searched, so run it from the vault (or name the folder):

```sh
$ topos --epub-links wiki Books
[[Moby Dick.epub#epubcfi(/6/14!/4/2/4,/1:0,/1:16)|John 3:16]]
$ topos --epub-links markdown -f abbrev .
[Jn 3:16](Books/Moby%20Dick.epub#epubcfi%28/6/14!/4/2/4,/1%3A0,/1%3A16%29)
```

Markdown destinations encode spaces, `(`, `)`, and `:`, as EPUB++ does. With `-m json`, each object
gets a `link` field instead; `-m count`, `-m total-count`, `-l`, and `--files` work as usual. CFIs
leave out `[id]` assertions unless `--cfi-assertions` is given, because `[` and `]` break
wikilinks.

Without links, `-m json` describes each reference in an EPUB with an `epub` object
(`spine_index`, `cfi`, and `chapter`, the table of contents' name for its content document), and
fills `line`, `utf16_column`, `start_utf16`, `end_utf16`, and `line_text` from the book's text:
each content document's text (block elements a blank line apart, as EPUB++ extracts it), one after
another with a blank line between. So positions are in book order, and `line_text` is the
paragraph. The Obsidian plugin's CLI engine uses these to search EPUBs.

## Usage

```
Find Bible references in files, directories, text, or stdin.

- Including a testament limits the search to it: `--nt -g Gospels` is the four Gospels
- Included genres and books add up: `-g Pentateuch -b Revelation` is six books
- Exclusions always win, so a book can be excluded from an included genre
- Passage filters that keep references (`-i`, `--any-overlap`, `-o`, `--exact-overlap`) are joined
 with a logical OR, then `--exclude-overlap` removes references

Usage: topos [OPTIONS] [PATHS]...

Arguments:
  [PATHS]...
          Files or directories to search (respecting .gitignore); defaults to stdin when piped, otherwise the current directory

Options:
      --text <TEXT>
          Search this text instead of files

  -t, --testament <TESTAMENTS>
          Include books from a testament (old/new)

      --nt
          Include the New Testament (same as `-t new`)

      --ot
          Include the Old Testament (same as `-t old`)

      --exclude-testament <EXCLUDE_TESTAMENTS>
          Exclude books from a testament

  -g, --genre <GENRES>
          Include books of a genre (e.g. epistles, gospels)

      --exclude-genre <EXCLUDE_GENRES>
          Exclude books of a genre

  -b, --book <BOOKS>
          Include a book (e.g. John)

      --exclude-book <EXCLUDE_BOOKS>
          Exclude a book

  -i, --inside <INSIDE>
          Only keep references entirely inside this passage (e.g. "John 1" keeps John 1:2-3)

      --any-overlap <ANY_OVERLAP>
          Only keep references that share any verse with this passage, whole chapters included (e.g. "John 3:16" keeps John 3 and John 3:14-18)

  -o, --explicit-overlap <EXPLICIT_OVERLAP>
          Only keep references that name a verse of this passage: whole chapters don't count (e.g. "John 3:16" keeps John 3:14-18 and John 2; 3:16, but not John 3)

      --exact-overlap <EXACT_OVERLAP>
          Only keep references that are exactly this passage, however they are written (e.g. "John 3:16-18" keeps Jn 3:16-18 and John 3:16, 17-18, but not John 3:16-17)

      --exclude-overlap <EXCLUDE_OVERLAP>
          Drop references that share any verse with this passage

      --context-book <CONTEXT_BOOK>
          Treat the input as being about this book, so references like 3:16 match

      --context-heading <CONTEXT_HEADING>
          Lines matching this pattern set the book for references after them, like '^#+ {book}$'

      --data <DATA>
          A JSON file with custom books, genres, or chapter and verse counts

      --merge-data <PATH>
          A JSON file (like --data, every field optional) whose names and values are added to the data, keeping the defaults: new abbreviations, books, genres, or chapter counts

      --remove-data <PATH>
          A JSON file (like --data, every field optional) whose values are removed from the data: a book or genre listed alone is removed entirely, otherwise just the values listed

  -q, --query <NAME>
          Use a named query from ~/.config/topos/queries.toml (its options go where this is)

      --list-queries
          List the named queries and exit

      --config <PATH>
          Read default options from this file instead of ~/.config/topos/config.toml

      --no-config
          Do not read default options from a config file

      --total-count
          Print only the total number of matches across all files (same as `-m total-count`)

  -m, --mode <MODE>
          How to print results

          Possible values:
          - auto:        Grouped by file on a terminal, otherwise `path:line:column: reference`
          - grouped:     Grouped by file, with optional context lines
          - quickfix:    `path:line:column: reference` (for Vim's quickfix list)
          - table:       A Markdown table
          - json:        One JSON object per match
          - count:       Matches per file
          - total-count: Matches across all files
          
          [default: auto]

  -f, --format <FORMAT>
          How to write each reference's book (also used by --complete); overrides --psg-fmt's `book` [default: name]

          Possible values:
          - name:         `Genesis 1:1`
          - abbreviation: `Gn 1:1`
          - osis:         `Gen.1.1`

      --psg-fmt <JSON>
          How to write references (results and completions), as JSON with any of: book, book_separator, chapter_verse, range, verse_separator, chapter_separator, omit_first_verse_of_chapter_range, join_adjacent, chapter_in_single_chapter_books. In config.toml it is a table: psg-fmt = { join_adjacent = true }

      --fmt-book-separator <TEXT>
          Between the book and its chapters [default: " "]

      --fmt-chapter-verse <TEXT>
          Between a chapter and a verse [default: ":"]

      --fmt-range <TEXT>
          Between the ends of a range [default: "-"]

      --fmt-verse-separator <TEXT>
          Before another verse in the same chapter [default: ","]

      --fmt-chapter-separator <TEXT>
          Before a part in another chapter [default: "; "]

      --fmt-join-adjacent [<BOOL>]
          Write adjacent verses as a range: 3:16-18 instead of 3:16,17,18 [default: false]
          
          [possible values: true, false]

      --fmt-omit-first-verse-of-chapter-range [<BOOL>]
          Write a range from a chapter's first verse as 1-2:3 instead of 1:1-2:3 [default: false]
          
          [possible values: true, false]

      --fmt-chapter-in-single-chapter-books [<BOOL>]
          Write the chapter in single-chapter books: Jude 1:5 instead of Jude 5 [default: true]
          
          [possible values: true, false]

      --epub-links <STYLE>
          Print an EPUB++ link to each reference instead (`[[Book.epub#epubcfi(...)|John 3:16]]`, or with markdown, `[John 3:16](Book.epub#epubcfi%28...%29)`), labeled with the reference as written by -f and --psg-fmt. Only EPUBs are searched, and the path in each link is relative to the directory searched (or just the file's name, for a file named on the command line). With -m json, each object gets a "link" field instead; -m count, -m total-count, -l, and --files work as usual

          Possible values:
          - wiki:     `[[Book.epub#epubcfi(...)|John 3:16]]`
          - markdown: `[John 3:16](Book.epub#epubcfi%28...%29)`

      --cfi-assertions
          Write EPUB CFIs with `[id]` assertions, like /6/14[chapter-1]!/4/2[p3]/1:0: more robust if the book changes, but `[` and `]` break wikilinks (EPUB++ leaves them out by default)

  -A, --after-context <AFTER_CONTEXT>
          Lines of context to show after each match
          
          [default: 0]

  -B, --before-context <BEFORE_CONTEXT>
          Lines of context to show before each match
          
          [default: 0]

  -C, --context <CONTEXT>
          Lines of context to show before and after each match

      --color <COLOR>
          When to use colors
          
          [default: auto]
          [possible values: auto, always, never]

      --sort
          Print results sorted by path (waits for the whole search)

      --cache
          Reuse results for files that have not changed (stored unfiltered, so any filters can use them)

      --no-cache
          Don't use the cache, even if the config turns it on

      --complete [<TEXT>]
          Print the completions for a partly typed reference, one per line, and exit: books for "" (or nothing), then chapters, verses, and range ends ("John 3:" gives John 3:1, ...). Uses -f for the book style and the book filters; -m json prints objects

      --list-books
          Print every book, one per line, and exit (the same as `--complete ""`)

      --clear-cache
          Delete the cache (in ~/.cache/topos) and exit

      --ext <EXT>
          Only search files with these extensions when walking directories (e.g. md,txt); files named on the command line are always searched

      --exclude-ext <EXT>
          Don't search files with these extensions when walking directories (e.g. pdf,epub)

      --glob <GLOB>
          Include or exclude paths matching a glob (`!` excludes), like `--glob '*.md' --glob '!drafts/**'`; can be repeated. Like ripgrep, globs override ignore files and --hidden

      --iglob <GLOB>
          Like --glob, ignoring case (when a path matches both, the --iglob wins)

  -., --hidden
          Search hidden files and directories (names starting with `.`)

      --no-ignore
          Don't respect ignore files (.gitignore, .ignore, .toposignore, and Git's global and exclude files)

      --no-ignore-vcs
          Don't respect Git's ignore files (.gitignore, the global gitignore, .git/info/exclude)

      --no-ignore-parent
          Don't respect ignore files in parent directories

      --no-require-git
          Respect .gitignore files outside of Git repositories too

      --ignore-file <PATH>
          Also ignore the paths in this file (gitignore syntax); can be repeated

  -u, --unrestricted...
          Search more: -u is --no-ignore, -uu adds --hidden, -uuu adds --binary

      --binary
          Search files that look binary as text (normally they are skipped)

  -L, --follow
          Follow symbolic links

  -d, --max-depth <NUM>
          Descend at most this many directories below the paths given (0 searches only them)

      --max-filesize <SIZE>
          Skip files larger than this, like 500K, 10M, or 1G

      --one-file-system
          Don't cross into other file systems (like mounted drives)

      --files
          Print the files that would be searched, without searching them

  -l, --files-with-matches
          Print only the paths of files with references

      --files-without-match
          Print only the paths of searched files without references

  -h, --help
          Print help (see a summary with '-h')

  -V, --version
          Print version
```
