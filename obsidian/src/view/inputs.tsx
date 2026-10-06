import { useEffect, useMemo, useRef, useState, type KeyboardEvent } from "react";
import { CompletionKind, type BookStyle, type Completion, type Topos } from "topos-bible";
import { applyCompletion, completionsBefore } from "../core/completions.ts";

interface Option {
  value: string;
  label: string;
  hint?: string;
}

/** Enough to list every verse of the longest chapter (Psalm 119), every chapter, or every book */
export const MAX_OPTIONS = 176;

/** A dropdown of suggestions under an input, driven by the keyboard or mouse */
function Dropdown({
  options,
  active,
  onPick,
}: {
  options: Option[];
  active: number;
  onPick: (index: number) => void;
}) {
  const activeItem = useRef<HTMLLIElement>(null);
  // Keep the keyboard's choice visible while scrolling through long lists
  useEffect(() => activeItem.current?.scrollIntoView({ block: "nearest" }), [active, options.length]);
  if (options.length === 0) return null;
  return (
    <ul className="topos-dropdown">
      {options.map((option, i) => (
        <li
          key={`${option.value}-${i}`}
          ref={i === active ? activeItem : undefined}
          className={`${i === active ? "is-active" : ""}${option.hint === "add" ? " is-add" : ""}`}
          // mousedown, not click, so the input keeps focus
          onMouseDown={(e) => {
            e.preventDefault();
            onPick(i);
          }}
        >
          <span>{option.label}</span>
          {option.hint && <span className="topos-suggestion-kind">{option.hint}</span>}
        </li>
      ))}
    </ul>
  );
}

/** Up and down move through the dropdown; returns true if the key was handled */
function moveActive(e: KeyboardEvent, count: number, setActive: (f: (i: number) => number) => void): boolean {
  if (count === 0) return false;
  if (e.key === "ArrowDown") setActive((i) => (i + 1) % count);
  else if (e.key === "ArrowUp") setActive((i) => (i - 1 + count) % count);
  else return false;
  e.preventDefault();
  return true;
}

/** A dropdown choice: add the typed reference, or complete it */
type Choice = { kind: "add"; reference: string } | { kind: "complete"; completion: Completion };

/**
 * The choices for what is typed: every book before anything is typed, then completions. A
 * finished reference can be added; that comes first unless the text ends in a delimiter (`3:`,
 * `16-`, `16,`), where the next number is probably wanted.
 */
export function referenceChoices(topos: Topos, style: BookStyle, value: string, caret: number): Choice[] {
  if (!value.trim()) {
    return topos.books().map((book) => {
      const name = book[(["name", "abbreviation", "osis"] as const)[style] ?? "name"] || book.name;
      return {
        kind: "complete",
        completion: { label: book.name, text: `${name} `, kind: CompletionKind.Book, start: 0, end: value.length },
      } as Choice;
    });
  }
  const passage = topos.parse(value, style);
  const completions = completionsBefore(topos, value.slice(0, caret), style, MAX_OPTIONS, "always")
    // Completing to what is already there does nothing
    .filter((c) => applyCompletion(value.slice(0, caret), c) + value.slice(caret) !== value);
  const choices: Choice[] = completions.map((completion) => ({ kind: "complete", completion }));
  if (!passage) return choices;
  // "John 3:16-" adds John 3:16, without the dangling dash
  const add: Choice = { kind: "add", reference: value.trim().replace(/[\s:\-–—,.;]+$/, "") };
  return /[:\-–—,.;]\s*$/.test(value) ? [...choices, add] : [add, ...choices];
}

/** An input for a Bible reference, with the same autocomplete as the editor */
export function ReferenceInput({
  topos,
  style,
  placeholder,
  onSubmit,
}: {
  topos: Topos;
  style: BookStyle;
  placeholder: string;
  onSubmit: (reference: string) => void;
}) {
  const [value, setValue] = useState("");
  const [caret, setCaret] = useState(0);
  const [active, setActive] = useState(0);
  const [focused, setFocused] = useState(false);
  const input = useRef<HTMLInputElement>(null);

  const choices = useMemo(() => referenceChoices(topos, style, value, caret), [topos, style, value, caret]);
  const valid = value.trim() !== "" && topos.parse(value, style) !== null;

  const update = (next: string, nextCaret = next.length) => {
    setValue(next);
    setCaret(nextCaret);
    setActive(0);
  };
  const choose = (choice: Choice | undefined) => {
    if (!choice) return;
    if (choice.kind === "add") {
      onSubmit(choice.reference);
      update("");
      return;
    }
    const { completion } = choice;
    const next = applyCompletion(value.slice(0, caret), completion) + value.slice(caret);
    const end = completion.start + completion.text.length;
    update(next, end);
    requestAnimationFrame(() => input.current?.setSelectionRange(end, end));
  };

  return (
    <div className="topos-input">
      <input
        ref={input}
        type="text"
        value={value}
        placeholder={placeholder}
        className={value && !valid ? "is-invalid" : ""}
        onChange={(e) => update(e.target.value, e.target.selectionStart ?? e.target.value.length)}
        onSelect={(e) => setCaret(e.currentTarget.selectionStart ?? value.length)}
        onFocus={() => setFocused(true)}
        onBlur={() => setFocused(false)}
        onKeyDown={(e) => {
          if (focused && moveActive(e, choices.length, setActive)) return;
          if (e.key === "Enter" || (e.key === "Tab" && choices[active])) {
            e.preventDefault();
            // Shift/Ctrl/Cmd-Enter always adds a finished reference
            if (e.key === "Enter" && valid && (e.shiftKey || e.ctrlKey || e.metaKey)) {
              choose(choices.find((c) => c.kind === "add"));
            } else choose(choices[active]);
          } else if (e.key === "Escape") {
            update("");
          }
        }}
      />
      {focused && (
        <Dropdown
          options={choices.map((choice) =>
            choice.kind === "add"
              ? { value: choice.reference, label: `Add "${choice.reference}"`, hint: "add" }
              : {
                  value: choice.completion.text,
                  label: choice.completion.label,
                  hint: ["book", "chapter", "verse"][choice.completion.kind],
                },
          )}
          active={active}
          onPick={(i) => choose(choices[i])}
        />
      )}
    </div>
  );
}

/** An input that picks from a list of names (books or genres), matching any alias */
export function NameInput({
  options,
  placeholder,
  onSubmit,
}: {
  options: { name: string; aliases: string[] }[];
  placeholder: string;
  onSubmit: (name: string) => void;
}) {
  const [value, setValue] = useState("");
  const [active, setActive] = useState(0);
  const [focused, setFocused] = useState(false);
  const matches = useMemo(() => {
    // Everything before anything is typed, so the whole list can be browsed
    const query = value.trim().toLowerCase();
    return options
      .filter((o) => !query || [o.name, ...o.aliases].some((alias) => alias.toLowerCase().startsWith(query)))
      .slice(0, MAX_OPTIONS);
  }, [options, value]);
  const add = (name: string) => {
    onSubmit(name);
    setValue("");
    setActive(0);
  };

  return (
    <div className="topos-input">
      <input
        type="text"
        value={value}
        placeholder={placeholder}
        onChange={(e) => {
          setValue(e.target.value);
          setActive(0);
        }}
        onFocus={() => setFocused(true)}
        onBlur={() => setFocused(false)}
        onKeyDown={(e) => {
          if (focused && moveActive(e, matches.length, setActive)) return;
          if ((e.key === "Enter" || e.key === "Tab") && matches[active]) {
            e.preventDefault();
            add(matches[active].name);
          } else if (e.key === "Escape") {
            setValue("");
          }
        }}
      />
      {focused && (
        <Dropdown
          options={matches.map((m) => ({ value: m.name, label: m.name, hint: m.aliases[0] }))}
          active={active}
          onPick={(i) => add(matches[i].name)}
        />
      )}
    </div>
  );
}

/** Removable values, like the CLI's repeated options */
export function Chips({ values, onRemove, exclude = false }: { values: string[]; onRemove: (i: number) => void; exclude?: boolean }) {
  return (
    <div className="topos-chips">
      {values.map((value, i) => (
        <span
          key={`${value}-${i}`}
          className={`topos-chip${exclude ? " is-exclude" : ""}`}
          title="Middle-click to remove"
          // Middle-click removes; mousedown is cancelled so it doesn't start autoscroll
          onMouseDown={(e) => {
            if (e.button === 1) e.preventDefault();
          }}
          onAuxClick={(e) => {
            if (e.button !== 1) return;
            e.preventDefault();
            onRemove(i);
          }}
        >
          {value}
          <button aria-label={`Remove ${value}`} onClick={() => onRemove(i)}>
            ×
          </button>
        </span>
      ))}
    </div>
  );
}
