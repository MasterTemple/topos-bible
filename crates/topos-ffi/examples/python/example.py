"""Run after `pip install dist/python/wheelhouse/*.whl` (see ../../README.md)."""

import pathlib
import tempfile

from topos_bible import BookStyle, OffsetUnit, Topos, ToposErrorException, ToposFiles, ToposOptions, ToposQuery

topos = Topos()

# Python strings index by code point, so ask for char offsets
text = "Notes 📖: read Jn 3:16-18 and Rom 8:28."
for m in topos.search(text, OffsetUnit.CHAR):
    print(f"{m.passage.reference} ({m.passage.osis}) at {m.start}..{m.end}:", text[m.start : m.end])

print(topos.parse("1 Cor 13:4-7", BookStyle.NAME).reference)

# Completions replace text[start:end] with completion.text
query = "see Gen 1:"
for c in topos.complete(query, len(query), OffsetUnit.CHAR, BookStyle.NAME, 3):
    print(c.kind.name, query[: c.start] + c.text + query[c.end :])

# Queries chain like the CLI's filter options; each method returns a new query
verses = "John 3, John 3:14-18, John 2; 3:16, Rom 8:28"
nt = ToposQuery.create().new_testament()
print([m.passage.reference for m in topos.search_with(verses, OffsetUnit.CHAR, nt.explicit_overlap("John 3:16"))])
print(topos.contradiction(ToposQuery.create().old_testament().genre("Pauline Epistles")))

# Options set up an instance: merge or remove data, or give bare references a book
custom = (
    ToposOptions.create()
    .merge_data('{"books":[{"book":"John","abbreviations":["jhn"]}]}')
    .remove_data('{"books":[{"book":"Jude"}]}')
    .build()
)
print([m.passage.reference for m in custom.search("Jhn 3:16 and Jude 5", OffsetUnit.CHAR)])

# Custom data (the same JSON as the CLI's --data); bad input raises
try:
    Topos.with_config("{")
except ToposErrorException as error:
    print("error:", error.error)

# Files and folders, like the CLI: its walking options, every format, and a cache in a folder
# of your choice (results are kept until a file changes)
with tempfile.TemporaryDirectory() as folder:
    notes = pathlib.Path(folder, "notes")
    notes.mkdir()
    (notes / "sermon.md").write_text("Text: Rom 8:28\nAlso Ps 23", encoding="utf-8")
    (notes / "draft.txt").write_text("Gen 1:1", encoding="utf-8")
    files = ToposFiles.create().extension("md").cache_dir(str(pathlib.Path(folder, "cache")))
    for found in topos.search_files([str(notes)], files, ToposQuery.create(), OffsetUnit.CHAR):
        print(pathlib.Path(found.path).name, [(m.passage.reference, m.line, m.column) for m in found.matches])
