# τόπος CLI

## Examples

*Results are truncated. These examples use `-m table`; on a terminal the default output groups results by file, and when piped it prints `path:line:column: reference`.*

## Install

```sh
cargo install --git https://github.com/MasterTemple/topos-bible topos-cli
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
topos -o "1 Peter 1:1-5, 4:11,14-16" -m table
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

`-o`/`--overlaps` keeps references that share any verse with the passage (like `1 Peter 4` above).
`-i`/`--inside` only keeps references entirely inside it, so `1 Peter 4` would be dropped.

### Exclude Testament/Genre/Book/Passage

Use just like above, but prefix the full option with `exclude` (or use `--outside` for passages)

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
| ./Church 07-20-25.md | 204  | 7   | Psalms 117        |
| ./Church 07-20-25.md | 208  | 12  | Psalms 117:1-2    |
| ./Church 07-20-25.md | 234  | 12  | Habakkuk 2:14     |
| ./Church 07-20-25.md | 255  | 12  | Psalms 117:2      |
```

## Rules

- Including a testament limits the search to it (`--nt -g Gospels` is the four Gospels)
- Included genres and books add up (`-g Pentateuch -b Revelation` is six books)
- Filters that can't match anything print a warning: genres or books outside the included
  testaments (`--ot -g "Pauline Epistles"`), `-i`/`-o` passages only in books that aren't searched
  (`-b Genesis -i "Romans 8"`), or only within `--outside` passages (`-i "John 3:16" --outside
  "John 3"`)
- Exclusions always win, so a book can be excluded from an included genre
- `-m count` prints matches per file; `--total-count` prints one total across all files
- `--inside` and `--overlaps` passages are joined with a logical OR, then `--outside` removes matches
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
```

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

`--cache` (or `cache = true` in `config.toml`) keeps each file's references between runs, until
the file changes (its size or modification time). Results are stored **before filtering**, so
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

## Shell completions

Tab completes options and their values: modes and formats, book and genre names (`-b jn` offers
Jonah and John; `-b "1 Co` gives `"1 Corinthians"`), testaments, your named queries, references
for `-i`/`-o`/`--outside` (books, then chapters, verses, and range ends, like the editor plugins),
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

## File types

- `.pdf` (with the `pdf` feature) reports the page, and `.epub` reports a CFI
- `.srt`, `.vtt`, and `.sbv` also report the cue's start time
- Everything else is searched as text; binary files and files ignored by `.gitignore` are skipped

## Usage

```
Find Bible references in files, directories, text, or stdin.

- Including a testament limits the search to it: `--nt -g Gospels` is the four Gospels
- Included genres and books add up: `-g Pentateuch -b Revelation` is six books
- Exclusions always win, so a book can be excluded from an included genre
- `--inside` and `--overlaps` passages are joined with a logical OR, then `--outside` removes matches

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

  -o, --overlaps <OVERLAPS>
          Only keep references that share any verse with this passage (e.g. "John 1" keeps John 1:51-2:1)

      --outside <OUTSIDE>
          Drop references that share any verse with this passage

      --context-book <CONTEXT_BOOK>
          Treat the input as being about this book, so references like 3:16 match

      --context-heading <CONTEXT_HEADING>
          Lines matching this pattern set the book for references after them, like '^#+ {book}$'

      --data <DATA>
          A JSON file with custom books, genres, or chapter and verse counts

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
          How to write each reference

          Possible values:
          - name:         `Genesis 1:1`
          - abbreviation: `Gn 1:1`
          - osis:         `Gen.1.1`
          
          [default: name]

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

      --clear-cache
          Delete the cache (in ~/.cache/topos) and exit

      --ext <EXT>
          Only search files with these extensions when walking directories (e.g. md,txt); files
          named on the command line are always searched

  -h, --help
          Print help (see a summary with '-h')

  -V, --version
          Print version
```
