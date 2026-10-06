"""Run after `pip install dist/python/wheelhouse/*.whl` (see ../../README.md)."""

from topos_bible import BookStyle, OffsetUnit, Topos, ToposErrorException

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

# Custom data (the same JSON as the CLI's --data); bad input raises
try:
    Topos.with_config("{")
except ToposErrorException as error:
    print("error:", error.error)
