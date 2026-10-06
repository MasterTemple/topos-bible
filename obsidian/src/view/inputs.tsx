import { useMemo, useRef, useState, type KeyboardEvent } from "react";
import type { BookStyle, Completion, Topos } from "topos-bible";
import { applyCompletion, completionsBefore } from "../core/completions.ts";

interface Option {
  value: string;
  label: string;
  hint?: string;
}

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
  if (options.length === 0) return null;
  return (
    <ul className="topos-dropdown">
      {options.map((option, i) => (
        <li
          key={`${option.value}-${i}`}
          className={i === active ? "is-active" : ""}
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

  const completions: Completion[] = useMemo(
    () => (value.trim() ? completionsBefore(topos, value.slice(0, caret), style, 12, "always") : []),
    [topos, style, value, caret],
  );
  const valid = value.trim() !== "" && topos.parse(value, style) !== null;

  const update = (next: string, nextCaret = next.length) => {
    setValue(next);
    setCaret(nextCaret);
    setActive(0);
  };
  const pick = (completion: Completion) => {
    const next = applyCompletion(value.slice(0, caret), completion) + value.slice(caret);
    update(next, completion.start + completion.text.length);
    requestAnimationFrame(() => {
      const end = completion.start + completion.text.length;
      input.current?.setSelectionRange(end, end);
    });
  };
  const submit = () => {
    if (!valid) return;
    onSubmit(value.trim());
    update("");
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
          if (focused && moveActive(e, completions.length, setActive)) return;
          if (e.key === "Tab" && completions[active]) {
            e.preventDefault();
            pick(completions[active]);
          } else if (e.key === "Enter") {
            e.preventDefault();
            // A finished reference is added; otherwise Enter completes
            if (valid && (completions.length === 0 || e.shiftKey || e.ctrlKey || e.metaKey)) submit();
            else if (completions[active]) pick(completions[active]);
            else submit();
          } else if (e.key === "Escape") {
            update("");
          }
        }}
      />
      {focused && (
        <Dropdown
          options={completions.map((c) => ({
            value: c.text,
            label: c.label,
            hint: ["book", "chapter", "verse"][c.kind],
          }))}
          active={active}
          onPick={(i) => pick(completions[i])}
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
    const query = value.trim().toLowerCase();
    if (!query) return [];
    return options
      .filter((o) => [o.name, ...o.aliases].some((alias) => alias.toLowerCase().startsWith(query)))
      .slice(0, 12);
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
        <span key={`${value}-${i}`} className={`topos-chip${exclude ? " is-exclude" : ""}`}>
          {value}
          <button aria-label={`Remove ${value}`} onClick={() => onRemove(i)}>
            ×
          </button>
        </span>
      ))}
    </div>
  );
}
