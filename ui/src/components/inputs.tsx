import { useEffect, useState } from "react";
import { EMPTY_DATE, nearestOption, parseFuzzy, scoreOptions, toFuzzy, today } from "../lib/format";

/**
 * A date where parts can be unknown: accepts `YYYY`, `YYYY-MM` or
 * `YYYY-MM-DD`. The value is Taiga's `YYYY-MM-DD` with `00` for unknown parts.
 */
export function FuzzyDateInput({
  id,
  value,
  onChange,
}: {
  id?: string;
  value: string;
  onChange: (value: string) => void;
}) {
  const [text, setText] = useState(() => display(value));
  const [invalid, setInvalid] = useState(false);
  useEffect(() => setText(display(value)), [value]);

  function commit(raw: string) {
    const trimmed = raw.trim();
    if (!trimmed) {
      setInvalid(false);
      onChange(EMPTY_DATE);
      return;
    }
    const match = /^(\d{4})(?:-(\d{1,2})(?:-(\d{1,2}))?)?$/.exec(trimmed);
    const parts = match && {
      year: Number(match[1]),
      month: Number(match[2] ?? 0),
      day: Number(match[3] ?? 0),
    };
    const valid = parts && parts.month <= 12 && parts.day <= 31 && (parts.month || !parts.day);
    setInvalid(!valid);
    if (valid && parts) onChange(toFuzzy(parts));
  }

  return (
    <span className="date-input">
      <input
        id={id}
        type="text"
        inputMode="numeric"
        placeholder="YYYY-MM-DD"
        value={text}
        aria-invalid={invalid}
        onChange={(e) => setText(e.target.value)}
        onBlur={(e) => commit(e.target.value)}
        onKeyDown={(e) => e.key === "Enter" && commit(e.currentTarget.value)}
      />
      <button type="button" className="ghost" onClick={() => onChange(today())}>
        Today
      </button>
      <button type="button" className="ghost" onClick={() => onChange(EMPTY_DATE)}>
        Clear
      </button>
    </span>
  );
}

function display(value: string): string {
  const { year, month, day } = parseFuzzy(value);
  if (!year) return "";
  const pad = (n: number) => String(n).padStart(2, "0");
  if (!month) return String(year);
  if (!day) return `${year}-${pad(month)}`;
  return `${year}-${pad(month)}-${pad(day)}`;
}

/** Score choices in the account's format; values are 0–100. */
export function ScoreSelect({
  id,
  value,
  format,
  onChange,
}: {
  id?: string;
  value: number;
  format: string | null;
  onChange: (value: number) => void;
}) {
  const options = scoreOptions(format);
  return (
    <select id={id} value={nearestOption(options, value)} onChange={(e) => onChange(Number(e.target.value))}>
      {options.map((o) => (
        <option key={o.value} value={o.value}>
          {o.label}
        </option>
      ))}
    </select>
  );
}

export function Toggle({
  label,
  description,
  checked,
  disabled,
  onChange,
}: {
  label: string;
  description?: string;
  checked: boolean;
  disabled?: boolean;
  onChange: (checked: boolean) => void;
}) {
  return (
    <label className={`toggle${disabled ? " disabled" : ""}`}>
      <input
        type="checkbox"
        checked={checked}
        disabled={disabled}
        onChange={(e) => onChange(e.target.checked)}
      />
      <span>
        {label}
        {description && <small className="muted">{description}</small>}
      </span>
    </label>
  );
}
