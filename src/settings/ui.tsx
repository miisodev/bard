import { useEffect, useRef, type InputHTMLAttributes, type KeyboardEvent, type ReactNode } from "react";

/*
  Settings primitives.

  The old settings sheet wrote every row by hand and reached for an inline
  style whenever something didn't quite fit — which is fine for nine settings
  and unworkable for sixty. Everything a pane needs is here, so a new setting
  is a line of markup rather than a small design decision taken again.

  Rule for anything added here: it takes its spacing, colour and type from the
  tokens, and it exposes no `style` prop. If a pane needs an escape hatch, the
  primitive is wrong.
*/

export function Pane({ title, description, children }: {
  title: string;
  description?: string;
  children: ReactNode;
}) {
  return (
    <div className="pane">
      <header className="pane-head">
        <h1>{title}</h1>
        {description ? <p>{description}</p> : null}
      </header>
      {children}
    </div>
  );
}

export function Group({ title, note, children }: {
  title?: string;
  /** Sits above the card. A note *inside* it reads as a setting, which it is not. */
  note?: ReactNode;
  children: ReactNode;
}) {
  return (
    <section className="group">
      {title ? <h2>{title}</h2> : null}
      {note ? <p className="group-note">{note}</p> : null}
      <div className="group-body">{children}</div>
    </section>
  );
}

/**
 * One setting: a label, an optional explanation, and its control.
 *
 * `htmlFor` makes the label a real `<label>` for the control it names, so
 * clicking "OpenRouter API key" puts the caret in the field. Without it the
 * label is a `<div>` that only looks like one.
 */
export function Row({ label, hint, children, stacked, htmlFor }: {
  label: ReactNode;
  hint?: ReactNode;
  children?: ReactNode;
  /** Put the control on its own line — for anything wider than a toggle. */
  stacked?: boolean;
  /** The id of the control in this row, so the label can name it. */
  htmlFor?: string;
}) {
  return (
    <div className={`row${stacked ? " stacked" : ""}`}>
      <div className="row-text">
        {htmlFor ? (
          <label className="row-label" htmlFor={htmlFor}>{label}</label>
        ) : (
          <div className="row-label">{label}</div>
        )}
        {hint ? <div className="row-hint">{hint}</div> : null}
      </div>
      {children ? <div className="row-control">{children}</div> : null}
    </div>
  );
}

export function Toggle({ checked, onChange, label, disabled = false }: {
  checked: boolean;
  onChange: (next: boolean) => void;
  label: string;
  disabled?: boolean;
}) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={checked}
      aria-label={label}
      className={`toggle${checked ? " on" : ""}`}
      disabled={disabled}
      onClick={() => onChange(!checked)}
    >
      <span className="knob" />
    </button>
  );
}

const SEGMENT_STEP: Record<string, number> = {
  ArrowRight: 1,
  ArrowDown: 1,
  ArrowLeft: -1,
  ArrowUp: -1,
};

/**
 * A radiogroup, which means more than the markup: one Tab stop, arrow keys to
 * move between options, and selection following focus. It was `role="radiogroup"`
 * with every option its own tab stop, so reaching "Forever" in the retention
 * rows cost six Tab presses and no arrow key did anything.
 */
export function Segmented<T extends string>({ value, options, onChange, label, disabled = false }: {
  value: T;
  options: { value: T; label: string }[];
  onChange: (next: T) => void;
  label: string;
  disabled?: boolean;
}) {
  const group = useRef<HTMLDivElement>(null);
  const selected = options.findIndex((o) => o.value === value);
  // With nothing selected — a value the control does not offer, which is what a
  // pane shows when it could not read the setting — the group still needs one
  // reachable option, or it is a keyboard dead end.
  const active = selected < 0 ? 0 : selected;

  const move = (next: number) => {
    if (options.length === 0) return;
    const index = (next + options.length) % options.length;
    onChange(options[index].value);
    group.current?.querySelectorAll<HTMLButtonElement>('[role="radio"]')[index]?.focus();
  };

  const onKeyDown = (event: KeyboardEvent<HTMLButtonElement>, index: number) => {
    const step = SEGMENT_STEP[event.key];
    if (step) {
      event.preventDefault();
      move(index + step);
    } else if (event.key === "Home") {
      event.preventDefault();
      move(0);
    } else if (event.key === "End") {
      event.preventDefault();
      move(options.length - 1);
    }
  };

  return (
    <div className="segmented" role="radiogroup" aria-label={label} ref={group}>
      {options.map((o, index) => (
        <button
          key={o.value}
          type="button"
          role="radio"
          aria-checked={value === o.value}
          tabIndex={index === active ? 0 : -1}
          className={value === o.value ? "on" : ""}
          disabled={disabled}
          onClick={() => onChange(o.value)}
          onKeyDown={(event) => onKeyDown(event, index)}
        >
          {o.label}
        </button>
      ))}
    </div>
  );
}

export function Select({ value, options, onChange, label, disabled = false }: {
  value: string;
  options: { value: string; label: string }[];
  onChange: (next: string) => void;
  label: string;
  disabled?: boolean;
}) {
  return (
    <div className="select-wrap">
      <select
        className="select"
        aria-label={label}
        value={value}
        disabled={disabled}
        onChange={(e) => onChange(e.target.value)}
      >
        {options.map((o) => (
          <option key={o.value} value={o.value}>{o.label}</option>
        ))}
      </select>
      <svg className="select-chevron" viewBox="0 0 16 16" aria-hidden="true">
        <path d="M4.5 6.5L8 10l3.5-3.5" fill="none" stroke="currentColor"
              strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" />
      </svg>
    </div>
  );
}

export function Button({ children, onClick, variant = "default", disabled, title, ariaLabel, ariaBusy }: {
  children: ReactNode;
  onClick: () => void;
  variant?: "default" | "primary" | "danger";
  disabled?: boolean;
  title?: string;
  ariaLabel?: string;
  ariaBusy?: boolean;
}) {
  return (
    <button
      type="button"
      className={`btn${variant === "default" ? "" : ` ${variant}`}`}
      onClick={onClick}
      disabled={disabled}
      title={title}
      aria-label={ariaLabel}
      aria-busy={ariaBusy || undefined}
    >
      {children}
    </button>
  );
}

type TextFieldProps = Pick<
  InputHTMLAttributes<HTMLInputElement>,
  "id" | "type" | "autoComplete" | "spellCheck" | "inputMode" | "disabled"
> & {
  value: string;
  onChange: (next: string) => void;
  placeholder?: string;
  onEnter?: () => void;
  label: string;
  showLabel?: boolean;
};

export function TextField({
  value,
  onChange,
  placeholder,
  onEnter,
  label,
  id,
  type = "text",
  autoComplete,
  spellCheck,
  inputMode,
  disabled,
  showLabel = false,
}: TextFieldProps) {
  const input = (
    <input
      id={id}
      className="field"
      aria-label={label}
      value={value}
      type={type}
      autoComplete={autoComplete}
      spellCheck={spellCheck}
      inputMode={inputMode}
      disabled={disabled}
      placeholder={placeholder}
      onChange={(e) => onChange(e.target.value)}
      onKeyDown={(e) => {
        if (e.key === "Enter" && onEnter && !disabled) onEnter();
      }}
    />
  );
  return showLabel ? (
    <label className="field-control" htmlFor={id}>
      <span className="field-label">{label}</span>
      {input}
    </label>
  ) : (
    input
  );
}

/** A short explanatory line under a group — never a caveat the UI should have solved. */
export function Note({ children, tone = "muted" }: {
  children: ReactNode;
  tone?: "muted" | "danger" | "success";
}) {
  return (
    <p
      className={`note${tone === "muted" ? "" : ` ${tone}`}`}
      role={tone === "danger" ? "alert" : undefined}
    >
      {children}
    </p>
  );
}

export function Progress({ percent, label, valueText }: {
  percent: number;
  label?: string;
  valueText?: string;
}) {
  const bounded = Math.max(0, Math.min(100, percent));
  return (
    <div
      className="progress"
      role="progressbar"
      aria-label={label}
      aria-valuenow={Math.round(bounded)}
      aria-valuemin={0}
      aria-valuemax={100}
      aria-valuetext={valueText}
    >
      <div className="fill" style={{ width: `${bounded}%` }} />
    </div>
  );
}

function qualitativeMeterValue(value: number): string {
  if (value < 25) return "Low";
  if (value < 50) return "Moderate";
  if (value < 75) return "High";
  return "Very high";
}

/**
 * A coarse relative reading, shown as a bar rather than a number because the
 * number would imply a precision these scores do not have. They rank Bard's
 * own choices against one another; they are not benchmark results.
 */
export function Meter({ label, value }: { label: string; value: number }) {
  const bounded = Math.max(0, Math.min(100, value));
  const qualitative = qualitativeMeterValue(bounded);
  return (
    <div className="meter">
      <span className="meter-label">{label}</span>
      <span
        className="meter-track"
        role="img"
        aria-label={`Relative ${label.toLowerCase()}: ${qualitative}`}
        title={`Relative ${label.toLowerCase()}: ${qualitative}`}
      >
        <span className="meter-fill" style={{ width: `${bounded}%` }} />
      </span>
    </div>
  );
}

/** Reads as a capability rather than a label: "English only", "Streaming". */
export function Tag({ children, tone = "default", title }: {
  children: ReactNode;
  tone?: "default" | "accent" | "success" | "danger" | "muted";
  title?: string;
}) {
  return (
    <span className={`tag${tone === "default" ? "" : ` ${tone}`}`} title={title}>
      {children}
    </span>
  );
}

export function formatBytes(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes <= 0) return "—";
  const gb = bytes / 1024 ** 3;
  if (gb >= 1) return `${Math.round(gb * 10) / 10} GB`;
  const mb = bytes / 1024 ** 2;
  // Rounding to whole megabytes reported "0 MB" for anything under half a
  // megabyte, so a 400 KB capture and a 2 MB one both claimed to be nothing.
  if (mb >= 1) return `${Math.round(mb * 10) / 10} MB`;
  const kb = bytes / 1024;
  if (kb >= 1) return `${Math.round(kb)} KB`;
  return `${Math.round(bytes)} B`;
}

/**
 * The one confirmation an irreversible action gets.
 *
 * It was `window.confirm` in the two places that mattered — clearing the whole
 * timeline and releasing a licence activation — which is an OS-drawn, locale-
 * varying, unstyleable dialog in a product that has decided not to use one, and
 * which cannot carry the sentence explaining what is about to happen.
 *
 * Escape cancels. `stopPropagation` matters: the settings window closes itself
 * on Escape, and a confirmation that closes the window it is asking about is
 * not a confirmation.
 */
export function InlineConfirm({
  children,
  confirmLabel,
  onCancel,
  onConfirm,
  confirming = false,
}: {
  children: ReactNode;
  confirmLabel: string;
  onCancel: () => void;
  onConfirm: () => void;
  confirming?: boolean;
}) {
  const bar = useRef<HTMLDivElement>(null);

  // The bar takes focus when it appears, so a keyboard user is told the
  // question exists rather than discovering it by tabbing into the answer.
  useEffect(() => {
    bar.current?.focus();
  }, []);

  return (
    <div
      ref={bar}
      className="remove-confirm"
      role="alert"
      tabIndex={-1}
      onKeyDown={(event) => {
        if (event.key !== "Escape") return;
        event.preventDefault();
        event.stopPropagation();
        onCancel();
      }}
    >
      <span>{children}</span>
      <div className="remove-confirm-actions">
        <Button onClick={onCancel} disabled={confirming}>Cancel</Button>
        <Button variant="danger" onClick={onConfirm} disabled={confirming} ariaBusy={confirming}>
          {confirmLabel}
        </Button>
      </div>
    </div>
  );
}

