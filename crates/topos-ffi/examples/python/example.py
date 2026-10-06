"""Run after `pip install dist/python/wheelhouse/*.whl` (see ../../README.md)."""

from topos_bible import BookStyle, OffsetUnit, Topos, ToposErrorException, ToposOptions, ToposQuery

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
