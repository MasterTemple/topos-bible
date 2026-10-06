// The shared completion cases (crates/topos-lib/tests/cases/complete.txt), through the plugin's
// completionsBefore, which the editor, the sidebar inputs, and the dialogs all use
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";
import { BookStyle, Topos } from "topos-bible";
import { completionsBefore } from "../src/core/completions.ts";
import { DEFAULT_FORMAT } from "../src/core/format.ts";

const cases = readFileSync(new URL("../../crates/topos-lib/tests/cases/complete.txt", import.meta.url), "utf8")
  .split("\n")
  .filter((line) => line.trim() && !line.startsWith("#"));
const labelCases = cases.filter((line) => !line.startsWith("apply: "));
const applyCases = cases.filter((line) => line.startsWith("apply: ")).map((line) => line.slice("apply: ".length));

test("completion matches the CLI, the language server, and the bindings", () => {
  const topos = Topos.new();
  const failures: string[] = [];
  for (const line of labelCases) {
    const [raw, expectedText] = line.split(" =>");
    const joinAdjacent = raw.startsWith("[join] ");
    const style = raw.startsWith("[abbreviation] ") ? BookStyle.Abbreviation : BookStyle.Name;
    const input = raw.replace(/^\[(join|abbreviation)\] /, "");
    const expected = expectedText.split(" | ").map((e) => e.trim()).filter(Boolean);
    const labels = completionsBefore(topos, input, style, 0, "always", {
      format: { ...DEFAULT_FORMAT, joinAdjacent },
    }).map((c) => c.label);
    const ok = expected.length === 0 ? labels.length === 0 : expected.every((e, i) => labels[i] === e);
    if (!ok) failures.push(`${JSON.stringify(input)}: expected ${JSON.stringify(expected)}, got ${JSON.stringify(labels.slice(0, 4))}`);
  }
  assert.deepEqual(failures, []);
});

test("completing mid-line doesn't double the space after a book (`apply:` cases)", () => {
  const topos = Topos.new();
  const failures: string[] = [];
  for (const line of applyCases) {
    const [input, expected] = line.split(" => ").map((part) => part.replaceAll("\\n", "\n"));
    const cursor = input.indexOf("|");
    const text = input.replace("|", "");
    const [first] = completionsBefore(topos, text.slice(0, cursor), BookStyle.Name, 0, "always", {
      after: text.slice(cursor),
    });
    const applied = first ? text.slice(0, first.start) + first.text + text.slice(first.end) : null;
    if (applied !== expected) failures.push(`${JSON.stringify(input)}: expected ${JSON.stringify(expected)}, got ${JSON.stringify(applied)}`);
  }
  assert.deepEqual(failures, []);
});
